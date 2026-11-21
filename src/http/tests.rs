use super::auth::{decode_jwt, encode_jwt, JwtClaims};
use chrono::Utc;

fn make_claims(exp_offset_secs: i64) -> JwtClaims {
    JwtClaims {
        sub: "user-123".into(),
        email: "test@example.com".into(),
        roles: vec!["editor".into()],
        exp: Utc::now().timestamp() + exp_offset_secs,
        ep: 0,
    }
}

#[test]
fn jwt_roundtrip() {
    let claims = make_claims(3600);
    let token = encode_jwt(&claims, "test-secret").unwrap();
    let decoded = decode_jwt(&token, "test-secret").unwrap();
    assert_eq!(decoded.sub, claims.sub);
    assert_eq!(decoded.email, claims.email);
    assert_eq!(decoded.roles, claims.roles);
}

#[test]
fn jwt_wrong_secret_rejected() {
    let claims = make_claims(3600);
    let token = encode_jwt(&claims, "correct-secret").unwrap();
    let result = decode_jwt(&token, "wrong-secret");
    assert!(result.is_err());
}

#[test]
fn jwt_expired_rejected() {
    let claims = make_claims(-1); // already expired
    let token = encode_jwt(&claims, "test-secret").unwrap();
    let result = decode_jwt(&token, "test-secret");
    assert!(result.is_err(), "expired token should be rejected");
}
