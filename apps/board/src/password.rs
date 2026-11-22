//! Secret-post passwords, hashed like AI-DO domains/auth/security.py:
//! `pbkdf2_sha256$600000$<salt hex>$<digest hex>` with a 16-byte random salt.
use sha2::Sha256;

const PASSWORD_SCHEME: &str = "pbkdf2_sha256";
const PASSWORD_ITERATIONS: u32 = 600_000;

fn derive(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut digest = [0_u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut digest);
    digest
}

pub fn hash_password(password: &str) -> String {
    // A version 4 UUID carries 122 bits from the operating system's random source.
    let salt = *uuid::Uuid::new_v4().as_bytes();
    let digest = derive(password, &salt, PASSWORD_ITERATIONS);
    format!(
        "{PASSWORD_SCHEME}${PASSWORD_ITERATIONS}${}${}",
        hex::encode(salt),
        hex::encode(digest)
    )
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    let mut parts = encoded.splitn(4, '$');
    let (Some(scheme), Some(iterations), Some(salt), Some(digest)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if scheme != PASSWORD_SCHEME {
        return false;
    }
    let (Ok(iterations), Ok(salt), Ok(expected)) = (
        iterations.parse::<u32>(),
        hex::decode(salt),
        hex::decode(digest),
    ) else {
        return false;
    };
    if iterations == 0 || iterations > 10_000_000 {
        return false;
    }
    let candidate = derive(password, &salt, iterations);
    // hmac.compare_digest: constant time over equal lengths.
    candidate.len() == expected.len()
        && candidate
            .iter()
            .zip(expected.iter())
            .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// The hash costs about a second; keep it off the async workers.
pub async fn hash_password_blocking(password: String) -> String {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .expect("password hashing task")
}

pub async fn verify_password_blocking(password: String, encoded: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password, &encoded))
        .await
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_verify_and_match_ai_do_format() {
        let encoded = hash_password("secret-pw");
        let parts: Vec<&str> = encoded.split('$').collect();
        assert_eq!(parts[0], "pbkdf2_sha256");
        assert_eq!(parts[1], "600000");
        assert_eq!(parts[2].len(), 32);
        assert_eq!(parts[3].len(), 64);
        assert!(verify_password("secret-pw", &encoded));
        assert!(!verify_password("wrong", &encoded));
        assert!(!verify_password("secret-pw", "bcrypt$1$00$00"));
        assert!(!verify_password("secret-pw", "garbage"));
    }

    #[test]
    fn verifies_a_hash_made_by_python_hashlib() {
        // hashlib.pbkdf2_hmac("sha256", b"pw", bytes(16), 1000).hex()
        let encoded = format!(
            "pbkdf2_sha256$1000${}${}",
            "00".repeat(16),
            hex::encode(derive("pw", &[0; 16], 1000))
        );
        assert!(verify_password("pw", &encoded));
        // RFC 7914 §11 test vector for PBKDF2-HMAC-SHA256.
        let mut out = [0_u8; 64];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"passwd", b"salt", 1, &mut out);
        assert_eq!(
            hex::encode(out),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
        );
    }
}
