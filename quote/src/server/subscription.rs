use chrono::Utc;
use crossbeam::channel::{Receiver, Sender, unbounded};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub enum SubscriptionManagerError {
    EmptyTopics,
    UnknownTopics { topics: Vec<String> },
    ShuttingDown,
}

pub struct Subscription<E> {
    pub session_id: Uuid,
    pub event_receiver: Receiver<(String, E)>,
}

struct SubscriptionSessionData<E> {
    topics_subscribed: HashSet<String>,
    event_sender: Sender<(String, E)>,
    last_ping_timestamp: AtomicI64,
}

struct SubscriptionManagerState<E> {
    active: AtomicBool,
    topics_supported: BTreeSet<String>,
    session_ping_timeout_ms: u32,
    sessions_by_id: RwLock<HashMap<Uuid, SubscriptionSessionData<E>>>,
}

#[derive(Clone)]
pub struct SubscriptionManager<E: Clone> {
    state: Arc<SubscriptionManagerState<E>>,
}

enum SendEventToSubscriberResult {
    Sent,
    NoTopicSubscription,
    SessionTimedOut,
    SessionReceiverClosed,
}

impl<E: Clone> SubscriptionManager<E> {
    pub fn new(topics: &Vec<String>, ping_timeout_ms: u32) -> Self {
        let mut topics_supported = BTreeSet::<String>::new();
        for topic in topics {
            topics_supported.insert(topic.clone());
        }

        let state = SubscriptionManagerState::<E> {
            active: AtomicBool::new(true),
            topics_supported,
            session_ping_timeout_ms: ping_timeout_ms,
            sessions_by_id: RwLock::new(HashMap::new()),
        };

        return Self {
            state: Arc::new(state),
        };
    }

    pub fn topics_supported(&self) -> &BTreeSet<String> {
        &self.state.topics_supported
    }

    pub fn subscribe(
        &mut self,
        topics: Vec<String>,
    ) -> Result<Subscription<E>, SubscriptionManagerError> {
        if !self.is_active() {
            return Err(SubscriptionManagerError::ShuttingDown);
        }

        if topics.is_empty() {
            return Err(SubscriptionManagerError::EmptyTopics);
        }

        let topics_supported = &self.state.topics_supported;
        let mut topics_subscribed = HashSet::new();
        let mut topics_unknown = Vec::new();

        for topic in topics.into_iter() {
            if !topics_supported.contains(&topic) {
                topics_unknown.push(topic);
            } else {
                topics_subscribed.insert(topic);
            }
        }

        if !topics_unknown.is_empty() {
            return Err(SubscriptionManagerError::UnknownTopics {
                topics: topics_unknown,
            });
        }

        let (event_sender, event_receiver) = unbounded();

        let session_data = SubscriptionSessionData {
            topics_subscribed,
            event_sender,
            last_ping_timestamp: AtomicI64::new(Utc::now().timestamp()),
        };

        let mut sessions_by_id = self.state.sessions_by_id.write().unwrap();

        if !self.is_active() {
            return Err(SubscriptionManagerError::ShuttingDown);
        }

        let session_id = Uuid::new_v4();

        _ = sessions_by_id.insert(session_id.clone(), session_data);

        return Ok(Subscription {
            session_id,
            event_receiver,
        });
    }

    pub fn refresh_subscription(
        &mut self,
        session_id: &Uuid,
    ) -> Result<bool, SubscriptionManagerError> {
        if !self.is_active() {
            return Err(SubscriptionManagerError::ShuttingDown);
        }

        let sessions_by_id = self.state.sessions_by_id.read().unwrap();

        if let Some(session_data) = sessions_by_id.get(session_id) {
            session_data
                .last_ping_timestamp
                .store(Utc::now().timestamp(), Ordering::SeqCst);
        } else {
            return Ok(false);
        }

        return Ok(true);
    }

    pub fn send_event(
        &mut self,
        topic: &String,
        event: &E,
    ) -> Result<usize, SubscriptionManagerError> {
        if !self.is_active() {
            return Err(SubscriptionManagerError::ShuttingDown);
        }

        let mut inactive_session_ids = Vec::new();

        let subscribers_notified =
            self.validate_sessions_send_event(topic, event, &mut inactive_session_ids);

        self.remove_inactive_sessions(&inactive_session_ids);

        return Ok(subscribers_notified);
    }

    fn validate_sessions_send_event(
        &self,
        topic: &String,
        event: &E,
        inactive_session_ids: &mut Vec<Uuid>,
    ) -> usize {
        let current_timestamp = Utc::now().timestamp();
        let mut subscribers_notified: usize = 0;

        let sessions_by_id = self.state.sessions_by_id.read().unwrap();

        for session_by_id in sessions_by_id.iter() {
            match self.send_event_to_subscriber(session_by_id.1, current_timestamp, topic, event) {
                SendEventToSubscriberResult::Sent => subscribers_notified += 1,
                SendEventToSubscriberResult::NoTopicSubscription => (),
                SendEventToSubscriberResult::SessionTimedOut
                | SendEventToSubscriberResult::SessionReceiverClosed => {
                    inactive_session_ids.push(session_by_id.0.clone());
                }
            }
        }

        return subscribers_notified;
    }

    fn send_event_to_subscriber(
        &self,
        session_data: &SubscriptionSessionData<E>,
        current_timestamp: i64,
        topic: &String,
        event: &E,
    ) -> SendEventToSubscriberResult {
        let last_ping_timestamp = session_data.last_ping_timestamp.load(Ordering::SeqCst);

        if current_timestamp - last_ping_timestamp > (self.state.session_ping_timeout_ms as i64) {
            return SendEventToSubscriberResult::SessionTimedOut;
        }

        if !session_data.topics_subscribed.contains(topic) {
            return SendEventToSubscriberResult::NoTopicSubscription;
        }

        if session_data
            .event_sender
            .send((topic.clone(), event.clone()))
            .is_err()
        {
            return SendEventToSubscriberResult::SessionReceiverClosed;
        }

        return SendEventToSubscriberResult::Sent;
    }

    fn remove_inactive_sessions(&mut self, inactive_session_ids: &Vec<Uuid>) {
        if inactive_session_ids.is_empty() {
            return;
        }

        let mut sessions_by_id = self.state.sessions_by_id.write().unwrap();

        for session_id in inactive_session_ids {
            sessions_by_id.remove(session_id);
        }
    }

    pub fn shut_down(&mut self) {
        if self
            .state
            .active
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.state.sessions_by_id.write().unwrap().clear();
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.active.load(Ordering::SeqCst)
    }
}
