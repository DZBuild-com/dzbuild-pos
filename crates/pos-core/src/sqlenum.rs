//! Stores small enums as their `as_str()` text in SQLite.

/// Implements `ToSql`/`FromSql` for a type with `as_str(self) -> &'static str`
/// and `parse(&str) -> Option<Self>`.
macro_rules! sql_text_enum {
    ($ty:ty) => {
        impl rusqlite::types::ToSql for $ty {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
            }
        }

        impl rusqlite::types::FromSql for $ty {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let text = value.as_str()?;
                <$ty>::parse(text).ok_or_else(|| {
                    rusqlite::types::FromSqlError::Other(
                        format!("invalid {} value: {text}", stringify!($ty)).into(),
                    )
                })
            }
        }
    };
}

pub(crate) use sql_text_enum;
