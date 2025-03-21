use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use diesel::{prelude::*, r2d2::ConnectionManager, MysqlConnection};
use r2d2::Pool;
use serde::Deserialize;
use serde_json::Value;

use super::new;
use crate::{
    errors::RvError,
    schema::{
        vault,
        vault::{dsl::*, vault_key},
    },
    storage::{Backend, BackendEntry},
};

pub struct MysqlBackend {
    pool: Arc<Mutex<Pool<ConnectionManager<MysqlConnection>>>>,
}

#[derive(Insertable, Queryable, PartialEq, Debug, Deserialize)]
#[diesel(table_name = vault)]
pub struct MysqlBackendEntry {
    pub vault_key: String,
    pub vault_value: Vec<u8>,
}

impl Backend for MysqlBackend {
    fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        if prefix.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendPrefixInvalid);
        }

        let conn: &mut MysqlConnection = &mut self.pool.lock().unwrap().get().unwrap();

        let results: Result<Vec<MysqlBackendEntry>, _> =
            vault.filter(vault_key.like(format!("{}%", prefix))).load::<MysqlBackendEntry>(conn);

        match results {
            Ok(entries) => {
                let mut keys: Vec<String> = Vec::new();
                for entry in entries {
                    let key = entry.vault_key.clone();
                    let key = key.trim_start_matches(prefix);
                    match key.find('/') {
                        Some(i) => {
                            let key = &key[0..i + 1];
                            if !keys.contains(&key.to_string()) {
                                keys.push(key.to_string());
                            }
                        }
                        None => {
                            keys.push(key.to_string());
                        }
                    }
                }
                return Ok(keys);
            }
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let conn: &mut MysqlConnection = &mut self.pool.lock().unwrap().get().unwrap();

        let result: Result<MysqlBackendEntry, _> = vault.filter(vault_key.eq(key)).first::<MysqlBackendEntry>(conn);

        match result {
            Ok(entry) => return Ok(Some(BackendEntry { key: entry.vault_key, value: entry.vault_value })),
            Err(e) => {
                if e == diesel::NotFound {
                    return Ok(None);
                } else {
                    return Err(RvError::ErrDatabaseExecuteEntry { source: (e) });
                }
            }
        }
    }

    fn put(&self, entry: &BackendEntry) -> Result<(), RvError> {
        if entry.key.as_str().starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let conn: &mut MysqlConnection = &mut self.pool.lock().unwrap().get().unwrap();

        let new_entry = MysqlBackendEntry { vault_key: entry.key.clone(), vault_value: entry.value.clone() };

        match diesel::replace_into(vault).values(&new_entry).execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    fn delete(&self, key: &str) -> Result<(), RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let conn: &mut MysqlConnection = &mut self.pool.lock().unwrap().get().unwrap();

        match diesel::delete(vault.filter(vault_key.eq(key))).execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }
}

impl MysqlBackend {
    pub fn new(conf: &HashMap<String, Value>) -> Result<MysqlBackend, RvError> {
        match new(conf) {
            Ok(pool) => Ok(MysqlBackend { pool: Arc::new(Mutex::new(pool)) }),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod test {

    use std::{collections::HashMap, env};

    use diesel::{prelude::*, MysqlConnection};
    use serde_json::Value;

    use super::MysqlBackend;

    use crate::errors::RvError;
    use crate::storage::test::{test_backend_curd, test_backend_list_prefix};

    fn mysql_table_clear(backend: &MysqlBackend) -> Result<(), RvError> {
        let conn: &mut MysqlConnection = &mut backend.pool.lock().unwrap().get().unwrap();

        match diesel::sql_query("TRUNCATE TABLE vault").execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    #[test]
    fn test_mysql_backend() {
        let mysql_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("password".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("address".to_string(), Value::String("127.0.0.1:3306".to_string()));
        conf.insert("username".to_string(), Value::String("root".to_string()));
        conf.insert("password".to_string(), Value::String(mysql_pwd));

        let backend = MysqlBackend::new(&conf);

        assert!(backend.is_ok());

        let backend = backend.unwrap();

        assert!(mysql_table_clear(&backend).is_ok());

        test_backend_curd(&backend);
        test_backend_list_prefix(&backend);
    }
}
