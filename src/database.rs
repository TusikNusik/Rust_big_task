use std::str;
use serde::{Serialize, Deserialize};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    let existing = sqlx::query("SELECT 1 FROM alerts WHERE user_id = ? AND symbol = ? AND direction = ? LIMIT 1")
        .bind(user_id)
        .bind(&alert.symbol)
        .bind(dir_str)
        .bind(alert.threshold)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB Error: {}", e))?;

    if existing.is_some() {
        return Err("Alert already exists".to_string());
    }

    sqlx::query("INSERT INTO alerts (user_id, symbol, direction, threshold) VALUES (?, ?, ?, ?)")
        .bind(user_id)
        .bind(&alert.symbol)
        .bind(dir_str)
        .bind(alert.threshold)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to add alert: {}", e))?;

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
        .map_err(|e| format!("Failed to fetch alerts: {}", e))?;

    let mut alerts = Vec::new();

    for row in rows {
        let dir_str: String = row.try_get("direction")
            .map_err(|e| format!("Failed to read row: {}", e))?;
        
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
        .map_err(|e| format!("Failed to remove the alert: {}", e))?;

    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize) ]
pub struct PortfolioStock {
    pub symbol: String,
    pub quantity: i32,
    pub total_price: f64,
}

pub async fn buy_stock(pool: &sqlx::SqlitePool, user_id: i64, symbol: &str, quantity: i32, current_price: f64) -> Result<(), String> {
    let stock_row = sqlx::query("SELECT quantity, price_total FROM positions WHERE user_id = ? AND symbol = ?")
        .bind(user_id)
        .bind(symbol)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(row) = stock_row {
        let current_quantity: i32 = row.try_get("quantity").unwrap_or(0);
        let current_summary: f64 = row.try_get("price_total").unwrap_or(0.0);
        
        let new_quantity = current_quantity + quantity;
        
        let total_value = current_summary + (quantity as f64 * current_price);

        sqlx::query("UPDATE positions SET quantity = ?, price_total = ? WHERE user_id = ? AND symbol = ?")
            .bind(new_quantity)
            .bind(total_value)
            .bind(user_id)
            .bind(symbol)
            .execute(pool).await.map_err(|e| e.to_string())?;

        Ok(())
    } 
    else {
       
        sqlx::query("INSERT INTO positions (user_id, symbol, quantity, price_total) VALUES (?, ?, ?, ?)")
            .bind(user_id)
            .bind(symbol)
            .bind(quantity)
            .bind(current_price * quantity as f64) // Twoja cena wejścia
            .execute(pool).await.map_err(|e| e.to_string())?;

        Ok(())
    }
}

pub async fn sell_stock(pool: &sqlx::SqlitePool, user_id: i64, symbol: &str, quantity: i32, stock_price: f64) -> Result<(), String> {
    
    let stock_row = sqlx::query("SELECT quantity, price_total FROM positions WHERE user_id = ? AND symbol = ?")
        .bind(user_id)
        .bind(symbol)
        .fetch_optional(pool)
        .await.map_err(|e| e.to_string())?;

    let (current_quantity, current_total_price): (i32, f64) = match stock_row {
        Some(row) => (row.try_get("quantity").unwrap_or(0), row.try_get("price_total").unwrap_or(0.0)),
        None => return Err("You have no stocks of this company.".to_string()),
    };

    if current_quantity < quantity {
        return Err(format!("You have only {} actions of given stock!.", current_quantity));
    }

    let new_quantity = current_quantity - quantity;
    let new_total_price = current_total_price - (quantity as f64 * stock_price);

    
    sqlx::query("UPDATE positions SET quantity = ?, price_total = ? WHERE user_id = ? AND symbol = ?")
        .bind(new_quantity).bind(new_total_price).bind(user_id).bind(symbol)
        .execute(pool).await.map_err(|e| e.to_string())?;
    

    Ok(())
}

pub async fn get_portfolio(pool: &sqlx::SqlitePool, user_id: i64) -> Result<Vec<PortfolioStock>, String> {
    let rows = sqlx::query("SELECT symbol, quantity, price_total FROM positions WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool).await.map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for row in rows {
        items.push(PortfolioStock {
            symbol: row.try_get("symbol").unwrap_or_default(),
            quantity: row.try_get("quantity").unwrap_or_default(),
            total_price: row.try_get("price_total").unwrap_or_default(),
        });
    }

    Ok(items)
}
