use sha2::{Sha256, Digest};
use rand::Rng;

pub fn generate_client_id(ip: &str) -> String {
    if let Ok(env) = std::env::var("NODE_ENV") {
        if env.to_lowercase() == "production" || env.to_lowercase() == "prod" {
            let salt1 = std::env::var("SALT1").unwrap_or_default();
            let salt2 = std::env::var("SALT2").unwrap_or_default();
            let salted = format!("{}{}{}", salt1, ip, salt2);
            
            let mut hasher = Sha256::new();
            hasher.update(salted.as_bytes());
            let result = hasher.finalize();
            
            return hex::encode(&result[..12]);
        }
    }
    
    generate_random_id()
}

pub fn generate_random_id() -> String {
    let bytes: Vec<u8> = (0..12)
        .map(|_| rand::thread_rng().gen())
        .collect();
    hex::encode(bytes)
}

pub fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}