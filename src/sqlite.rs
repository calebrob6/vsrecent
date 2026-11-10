use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs;
use std::path::{Path, PathBuf};

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;
const SQLITE_OPEN_NOMUTEX: c_int = 0x0000_8000;

// Both platforms ship SQLite: `winsqlite3.dll` on Windows and `libsqlite3.dylib` on macOS.
#[cfg_attr(windows, link(name = "winsqlite3"))]
#[cfg_attr(not(windows), link(name = "sqlite3"))]
unsafe extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        db: *mut *mut c_void,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        bytes: c_int,
        statement: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(statement: *mut c_void) -> c_int;
    fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const u8;
    fn sqlite3_column_bytes(statement: *mut c_void, column: c_int) -> c_int;
    fn sqlite3_finalize(statement: *mut c_void) -> c_int;
    fn sqlite3_errmsg(db: *mut c_void) -> *const c_char;
}

pub fn read_recent_json(db_path: &Path) -> Result<Option<String>, String> {
    const SQL: &str =
        "SELECT value FROM ItemTable WHERE key='history.recentlyOpenedPathsList' LIMIT 1";

    match read_single_text(db_path, SQL) {
        Ok(value) => Ok(value),
        Err(first_error) => {
            let snapshot = Snapshot::create(db_path)
                .map_err(|error| format!("{first_error}; snapshot failed: {error}"))?;
            read_single_text(&snapshot.db_path, SQL)
        }
    }
}

fn read_single_text(path: &Path, sql: &str) -> Result<Option<String>, String> {
    let path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "database path contains a NUL byte".to_string())?;
    let sql = CString::new(sql).expect("static SQL has no NUL bytes");
    let mut db = std::ptr::null_mut();
    let mut statement = std::ptr::null_mut();

    let result = unsafe {
        let open_result = sqlite3_open_v2(
            path.as_ptr(),
            &mut db,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX,
            std::ptr::null(),
        );
        if open_result != SQLITE_OK {
            Err(format!(
                "could not open VS Code history database ({open_result})"
            ))
        } else {
            let prepare_result =
                sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut());
            if prepare_result != SQLITE_OK {
                Err(error_message(db, "could not read VS Code history"))
            } else {
                match sqlite3_step(statement) {
                    SQLITE_ROW => {
                        let bytes = sqlite3_column_bytes(statement, 0);
                        let text = sqlite3_column_text(statement, 0);
                        if text.is_null() || bytes <= 0 {
                            Ok(Some(String::new()))
                        } else {
                            let slice = std::slice::from_raw_parts(text, bytes as usize);
                            String::from_utf8(slice.to_vec())
                                .map(Some)
                                .map_err(|error| format!("history is not UTF-8: {error}"))
                        }
                    }
                    SQLITE_DONE => Ok(None),
                    _ => Err(error_message(db, "could not read live VS Code history")),
                }
            }
        }
    };

    unsafe {
        if !statement.is_null() {
            sqlite3_finalize(statement);
        }
        if !db.is_null() {
            sqlite3_close(db);
        }
    }
    result
}

unsafe fn error_message(db: *mut c_void, context: &str) -> String {
    let pointer = unsafe { sqlite3_errmsg(db) };
    if pointer.is_null() {
        context.to_string()
    } else {
        let detail = unsafe { CStr::from_ptr(pointer) }.to_string_lossy();
        format!("{context}: {detail}")
    }
}

struct Snapshot {
    directory: PathBuf,
    db_path: PathBuf,
}

impl Snapshot {
    fn create(source: &Path) -> Result<Self, String> {
        let directory = std::env::temp_dir().join(format!("vsrecent_{}", std::process::id()));
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let db_path = directory.join("state.vscdb");
        fs::copy(source, &db_path).map_err(|error| error.to_string())?;

        for suffix in ["-wal", "-shm"] {
            let source_sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
            if source_sidecar.exists() {
                let destination = PathBuf::from(format!("{}{suffix}", db_path.display()));
                fs::copy(source_sidecar, destination).map_err(|error| error.to_string())?;
            }
        }
        Ok(Self { directory, db_path })
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
    const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
    const CREATE_ITEM_TABLE: &str =
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)";

    /// A database in its own temporary directory, laid out like VS Code's shared storage.
    struct TestDatabase {
        directory: PathBuf,
        path: PathBuf,
    }

    impl TestDatabase {
        fn create(name: &str, statements: &[&str]) -> Self {
            let directory =
                std::env::temp_dir().join(format!("vsrecent_test_{}_{name}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join("state.vscdb");
            let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
            let mut db = std::ptr::null_mut();
            unsafe {
                let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE;
                let opened = sqlite3_open_v2(c_path.as_ptr(), &mut db, flags, std::ptr::null());
                assert_eq!(opened, SQLITE_OK);
                for statement in statements {
                    let sql = CString::new(*statement).unwrap();
                    let mut prepared = std::ptr::null_mut();
                    let result = sqlite3_prepare_v2(
                        db,
                        sql.as_ptr(),
                        -1,
                        &mut prepared,
                        std::ptr::null_mut(),
                    );
                    assert_eq!(result, SQLITE_OK, "{statement}");
                    let step = sqlite3_step(prepared);
                    assert!(step == SQLITE_DONE || step == SQLITE_ROW, "{statement}");
                    sqlite3_finalize(prepared);
                }
                sqlite3_close(db);
            }
            Self { directory, path }
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn reads_history_from_a_wal_database() {
        let json = r#"{"entries":[{"folderUri":"file:///code/project"}]}"#;
        let insert =
            format!("INSERT INTO ItemTable VALUES ('history.recentlyOpenedPathsList', '{json}')");
        let database = TestDatabase::create(
            "wal",
            &["PRAGMA journal_mode=WAL", CREATE_ITEM_TABLE, &insert],
        );
        assert_eq!(
            read_recent_json(&database.path).unwrap().as_deref(),
            Some(json)
        );
    }

    #[test]
    fn reports_absent_history() {
        let database = TestDatabase::create("absent", &[CREATE_ITEM_TABLE]);
        assert_eq!(read_recent_json(&database.path).unwrap(), None);
    }
}
