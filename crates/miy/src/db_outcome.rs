//! Proof that PostgreSQL rejected a write (V1-O4).
//!
//! Only a server response with one of these SQLSTATEs proves the statement or
//! transaction did not commit. Connection, I/O and pool errors, other SQLSTATEs and
//! class 40 as a whole (it includes 40003 statement_completion_unknown) leave the
//! outcome unknown. An unknown outcome never authorizes stopping, deleting or
//! revoking something that a committed row may still describe.
const REJECTED: [&str; 7] = [
    "23502", "23503", "23505", "23514", "23P01", // integrity violations
    "40001", "40P01", // serialization failure, deadlock: rolled back
];

/// The error chain holds a database response that proves the write was rejected.
pub(crate) fn rejected(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<sqlx::Error>(),
            Some(sqlx::Error::Database(db))
                if db.code().is_some_and(|code| REJECTED.contains(&code.as_ref()))
        )
    })
}

/// A database operation failed without proof that it was rejected. A failed connect
/// that was already classified (for example `database_unavailable`) wrote nothing and
/// is not an uncertain outcome.
pub(crate) fn uncertain(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<sqlx::Error>().is_some())
        && !rejected(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug)]
    struct Response(&'static str);
    impl std::fmt::Display for Response {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.0)
        }
    }
    impl std::error::Error for Response {}
    impl sqlx::error::DatabaseError for Response {
        fn message(&self) -> &str {
            self.0
        }
        fn code(&self) -> Option<Cow<'_, str>> {
            Some(Cow::Borrowed(self.0))
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    fn response(code: &'static str) -> anyhow::Error {
        anyhow::Error::from(sqlx::Error::Database(Box::new(Response(code)))).context("step")
    }

    #[test]
    fn only_listed_responses_prove_rejection() {
        for code in REJECTED {
            assert!(
                rejected(&response(code)) && !uncertain(&response(code)),
                "{code}"
            );
        }
        for code in [
            "40003", "40002", "08006", "57P01", "53300", "XX000", "42501",
        ] {
            assert!(
                !rejected(&response(code)) && uncertain(&response(code)),
                "{code}"
            );
        }
    }

    #[test]
    fn transport_failures_are_uncertain_and_classified_failures_are_not() {
        let io = anyhow::Error::from(sqlx::Error::Io(std::io::ErrorKind::ConnectionReset.into()));
        assert!(uncertain(&io) && !rejected(&io));
        assert!(uncertain(&anyhow::Error::from(sqlx::Error::PoolTimedOut)));
        use crate::runtime_contract::{FailureCode, RuntimeFailure};
        for code in [
            FailureCode::DatabaseUnavailable,
            FailureCode::DockerUnavailable,
        ] {
            let classified = anyhow::Error::from(RuntimeFailure::new(code));
            assert!(!uncertain(&classified) && !rejected(&classified));
        }
        assert!(!uncertain(&anyhow::anyhow!("app_health_failed")));
    }
}
