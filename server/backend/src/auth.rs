use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT Claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64,       // user id
    pub username: String,
    pub role_code: String,
    pub exp: usize,     // 过期时间戳
}

/// 登录请求
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 登录响应
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

/// 用户信息（返回给前端）
#[derive(Serialize, Clone)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub real_name: String,
    pub role_code: String,
    pub role_name: String,
}

/// 用 Argon2id 哈希密码
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("hash error: {e}"))?
        .to_string();
    Ok(hash)
}

/// 验证密码
pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(hash).map_err(|e| format!("parse hash error: {e}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// 生成 JWT token（有效期 24 小时）
pub fn generate_token(claims: &Claims, secret: &str) -> Result<String, String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| format!("token error: {e}"))
}

/// 验证 JWT token，返回 Claims
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, String> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| format!("token invalid: {e}"))?;
    Ok(token_data.claims)
}

/// 创建 Claims
pub fn create_claims(user_id: i64, username: String, role_code: String) -> Claims {
    let exp = (Utc::now() + Duration::hours(24)).timestamp() as usize;
    Claims {
        sub: user_id,
        username,
        role_code,
        exp,
    }
}
