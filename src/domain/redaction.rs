/// Field-name policy for serialized diagnostics. Token usage remains visible.
pub fn sensitive_field(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "password"
            | "token"
            | "authorization"
            | "api_key"
            | "secret"
            | "private_key"
            | "mnemonic"
            | "seed"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "proxy-authorization"
    )
}
