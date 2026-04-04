//! Модуль, определяющий структуру транзакции и связанные с ней перечисления.

use derive_getters::Getters;

/// Типы банковских транзакций.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransactionType {
    /// Пополнение счёта.
    Deposit,
    /// Перевод средств между пользователями.
    Transfer,
    /// Снятие средств со счёта.
    Withdrawal,
}

/// Статусы выполнения транзакции.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransactionStatus {
    /// Транзакция успешно выполнена.
    Success,
    /// Транзакция завершилась ошибкой.
    Failure,
    /// Транзакция ожидает обработки.
    Pending,
}

/// финансовая транзакция.
///
/// Содержит информацию о транзакции, включая идентификаторы
/// участников, сумму, статус и описание.
#[derive(Getters, PartialEq, Debug)]
pub struct Transaction {
    /// ID транзакции.
    id: u64,

    /// Тип транзакции (депозит, перевод, снятие).    
    #[getter(copy)]
    tx_type: TransactionType,
    /// Текущий статус транзакции.
    #[getter(copy)]
    tx_status: TransactionStatus,

    /// Время выполнения транзакции (Unix time).
    timestamp: u64,
    /// Идентификатор пользователя-отправителя.
    from_user_id: u64,
    /// Идентификатор пользователя-получателя.
    to_user_id: u64,
    /// Сумма транзакции.
    amount: u64,

    /// Описание транзакции.
    description: String,
}

impl Transaction {
    /// Создаёт новый экземпляр транзакции.
    ///
    /// # Аргументы
    /// * `id` - ID транзакции
    /// * `tx_type` - Тип транзакции
    /// * `tx_status` - Статус транзакции
    /// * `timestamp` - Время создания
    /// * `from_user_id` - ID отправителя
    /// * `to_user_id` - ID получателя
    /// * `amount` - Сумма
    /// * `description` - Описание
    pub fn new(
        id: u64,
        tx_type: TransactionType,
        tx_status: TransactionStatus,
        timestamp: u64,
        from_user_id: u64,
        to_user_id: u64,
        amount: u64,
        description: String,
    ) -> Self {
        Self {
            id: id,
            tx_type: tx_type,
            tx_status: tx_status,
            timestamp: timestamp,
            from_user_id: from_user_id,
            to_user_id: to_user_id,
            amount: amount,
            description: description,
        }
    }
}
