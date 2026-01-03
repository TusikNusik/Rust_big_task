use sqlx::{sqlite, Row};
use crate::protocol::{AlertDirection, AlertRequest};

use argon2::{
    password_hash::{
        rand_core::OsRng,
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString
    },
    Argon2
};

// Struktura pomocnicza do wyciągania danych
#[derive(Debug)]
pub struct StoredAlert {
    pub symbol: String,
    pub direction: AlertDirection,
    pub threshold: f64,
}

pub async fn init_database(pool: &sqlite::SqlitePool) -> Result<(), String> {
    let database = include_str!("querys.sql"); 

    sqlx::query(database)
        .execute(pool)
        .await
        .map_err(|e| format!("Init DB error: {}", e))?;

    Ok(())
}

pub async fn add_alert(pool: &sqlite::SqlitePool, user_id : i64, alert : &AlertRequest) -> Result<(), String> {
    let dir_str = alert.direction.as_str();

    sqlx::query("INSERT INTO alerts (user_id, symbol, direction, threshold) VALUES (?, ?, ?, ?)")
        .bind(user_id)
        .bind(&alert.symbol)
        .bind(dir_str)
        .bind(alert.threshold)
        .execute(pool)
        .await
        .map_err(|e| format!("DB Error: {}", e))?;

    println!("[database] Succesfully added new alert!");
    Ok(())
}

pub async fn register_user(pool: &sqlite::SqlitePool, username : &str, password : &str) -> Result<(), String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();

    let register_result = sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(username)
        .bind(password_hash)
        .execute(pool)
        .await;

    match register_result {
        Ok(_) =>  {
            println!("[database] Succesfully registered new user!");
            Ok(())
        },
        Err(sqlx::Error::Database(db_err)) if db_err.message().contains("UNIQUE constraint") => {
            Err("User already exists".to_string())
        },
        Err(e) => {
            Err(format!("Database error: {}", e))
        }
    }
}

pub async fn login_user(pool: &sqlite::SqlitePool, username : &str, password : &str) -> Result<i64, String> {
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(row) = row {
        let stored_hash: String = row.try_get("password_hash").map_err(|e| e.to_string())?;
        let user_id: i64 = row.try_get("id").map_err(|e| e.to_string())?;

        let parsed_hash = PasswordHash::new(&stored_hash).map_err(|e| e.to_string())?;
        
        if Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok() {
            return Ok(user_id);
        }
    }

    Err("Invalid username or password".to_string())
}

pub async fn get_user_alerts(pool: &sqlx::SqlitePool, user_id: i64) -> Result<Vec<StoredAlert>, String> {
    let rows = sqlx::query("SELECT symbol, direction, threshold FROM alerts WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Błąd pobierania alertów: {}", e))?;

    let mut alerts = Vec::new();

    for row in rows {
        let dir_str: String = row.try_get("direction")
            .map_err(|e| format!("Błąd odczytu kolumny direction: {}", e))?;
        
        if let Some(direction) = AlertDirection::as_msg(&dir_str) {
            alerts.push(StoredAlert {
                symbol: row.try_get("symbol").unwrap_or_default(),
                threshold: row.try_get("threshold").unwrap_or_default(),
                direction,
            });
        }
    }

    Ok(alerts)
}

pub async fn remove_alert(
    pool: &sqlx::SqlitePool, 
    user_id: i64, 
    symbol: &str, 
    direction: AlertDirection
) -> Result<(), String> {
    
    let dir_str = direction.as_str(); 

    sqlx::query("DELETE FROM alerts WHERE user_id = ? AND symbol = ? AND direction = ?")
        .bind(user_id)
        .bind(symbol)
        .bind(dir_str)
        .execute(pool)
        .await
        .map_err(|e| format!("Błąd usuwania alertu: {}", e))?;

    Ok(())
}