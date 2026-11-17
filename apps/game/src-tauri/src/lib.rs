use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchRecord {
    id: String,
    finished_at: String,
    winner: String,
    summary: String,
}

fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS matches (id TEXT PRIMARY KEY, finished_at TEXT NOT NULL, winner TEXT NOT NULL, summary TEXT NOT NULL)")
}

fn database(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let conn = Connection::open(dir.join("matches.sqlite3")).map_err(|e| e.to_string())?;
    initialize(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

fn insert_match(conn: &Connection, record: &MatchRecord) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO matches (id, finished_at, winner, summary) VALUES (?1, ?2, ?3, ?4)",
        params![record.id, record.finished_at, record.winner, record.summary],
    )?;
    Ok(())
}

#[tauri::command]
fn save_match(app: tauri::AppHandle, record: MatchRecord) -> Result<(), String> {
    insert_match(&database(&app)?, &record).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_matches(app: tauri::AppHandle) -> Result<Vec<MatchRecord>, String> {
    let conn = database(&app)?;
    let mut stmt = conn.prepare("SELECT id, finished_at, winner, summary FROM matches ORDER BY finished_at DESC, rowid DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(MatchRecord {
        id: row.get(0)?, finished_at: row.get(1)?, winner: row.get(2)?, summary: row.get(3)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![save_match, list_matches])
        .run(tauri::generate_context!())
        .expect("failed to run application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_completion_does_not_duplicate_or_replace_match() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        let mut record = MatchRecord { id: "match-1".into(), finished_at: "2026-11-18T00:00:00Z".into(), winner: "A".into(), summary: "first".into() };
        insert_match(&conn, &record).unwrap();
        record.winner = "B".into();
        insert_match(&conn, &record).unwrap();
        let (count, winner): (i64, String) = conn.query_row("SELECT count(*), winner FROM matches", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!((count, winner.as_str()), (1, "A"));
    }
}
