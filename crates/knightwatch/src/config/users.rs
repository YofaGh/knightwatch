use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use super::store::JsonStore;
use crate::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub telegram_token: String,
    pub telegram_chat_id: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Users {
    pub users: Vec<User>,
}

impl JsonStore for Users {
    const NAME: &'static str = "users";
    fn path() -> std::path::PathBuf {
        super::paths::data_file_path("users.json")
    }
}

impl Users {
    pub fn find(&self, username: &str) -> Option<&User> {
        self.users.iter().find(|u| u.username == username)
    }

    pub fn find_by_telegram_token_mut(&mut self, token: &str) -> Option<&mut User> {
        self.users.iter_mut().find(|u| u.telegram_token == token)
    }

    pub fn get_telegram_chat_ids(&self) -> Vec<i64> {
        self.users.iter().filter_map(|u| u.telegram_chat_id).collect()
    }

    pub fn add(&mut self, user: User) -> Result<()> {
        if self.find(&user.username).is_some() {
            return Err(Error::Config(format!(
                "User '{}' already exists",
                user.username
            )));
        }
        self.users.push(user);
        Ok(())
    }

    pub fn remove(&mut self, username: &str) -> Result<()> {
        if self.find(username).is_none() {
            return Err(Error::Config(format!("User '{}' not found", username)));
        }
        self.users.retain(|u| u.username != username);
        Ok(())
    }

    pub fn verify_password(&self, username: &str, password: &str) -> Result<bool> {
        let user = self
            .find(username)
            .ok_or_else(|| Error::Config(format!("User '{username}' not found")))?;
        bcrypt::verify(password, &user.password_hash)
            .map_err(|e| Error::Config(format!("Failed to verify password: {e}")))
    }
}

pub fn generate_telegram_token() -> String {
    let mut rng = rand::rng();
    let number = rand::RngExt::random_range(&mut rng, 100_000..=99_999_999);
    number.to_string()
}

pub fn hash_password(password: &str) -> Result<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| Error::Config(format!("Failed to hash password: {e}")))
}

static USERS: OnceLock<Users> = OnceLock::new();

pub fn get_users() -> &'static Users {
    USERS.get().expect("Users not initialized")
}

pub fn load_users() -> Result<()> {
    let users = Users::load()?;
    USERS
        .set(users)
        .map_err(|_| Error::Config("Users already initialized".to_string()))?;
    Ok(())
}

pub fn set_user_chat_id(token: String, chat_id: i64) -> Result<bool> {
    let mut users = Users::load()?;
    if let Some(user) = users.find_by_telegram_token_mut(&token) {
        user.telegram_chat_id = Some(chat_id);
        users.save()?;
        Ok(true)
    } else {
        Ok(false)
    }
}
