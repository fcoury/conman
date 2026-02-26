use std::net::SocketAddr;

use crate::ConmanError;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub mongo_uri: String,
    pub mongo_db: String,
    pub gitaly_address: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub invite_expiry_days: u64,
    pub secrets_master_key: String,
    pub temp_url_domain: String,
    pub http_rate_limit_per_second: u64,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from_email: Option<String>,
    pub msuite_submit_cmd: String,
    pub msuite_merge_cmd: String,
    pub msuite_deploy_cmd: String,
    pub deploy_release_cmd: String,
    pub runtime_profile_drift_check_cmd: String,
    pub temp_env_provision_cmd: Option<String>,
    pub temp_env_expire_cmd: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConmanError> {
        let host = std::env::var("CONMAN_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("CONMAN_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_PORT must be a valid u16".to_string(),
            })?;

        let listen_addr =
            format!("{host}:{port}")
                .parse()
                .map_err(|_| ConmanError::Validation {
                    message: "CONMAN_HOST:CONMAN_PORT must form a valid socket address".to_string(),
                })?;

        let jwt_secret =
            std::env::var("CONMAN_JWT_SECRET").map_err(|_| ConmanError::Validation {
                message: "CONMAN_JWT_SECRET is required".to_string(),
            })?;
        if jwt_secret.len() < 32 {
            return Err(ConmanError::Validation {
                message: "CONMAN_JWT_SECRET must be at least 32 characters".to_string(),
            });
        }

        let jwt_expiry_hours: u64 = std::env::var("CONMAN_JWT_EXPIRY_HOURS")
            .unwrap_or_else(|_| "24".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_JWT_EXPIRY_HOURS must be a valid u64".to_string(),
            })?;

        let invite_expiry_days: u64 = std::env::var("CONMAN_INVITE_EXPIRY_DAYS")
            .unwrap_or_else(|_| "7".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_INVITE_EXPIRY_DAYS must be a valid u64".to_string(),
            })?;

        let secrets_master_key =
            std::env::var("CONMAN_SECRETS_MASTER_KEY").map_err(|_| ConmanError::Validation {
                message: "CONMAN_SECRETS_MASTER_KEY is required".to_string(),
            })?;

        let temp_url_domain =
            std::env::var("CONMAN_TEMP_URL_DOMAIN").map_err(|_| ConmanError::Validation {
                message: "CONMAN_TEMP_URL_DOMAIN is required".to_string(),
            })?;

        let http_rate_limit_per_second: u64 = std::env::var("CONMAN_HTTP_RATE_LIMIT_PER_SECOND")
            .unwrap_or_else(|_| "200".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_HTTP_RATE_LIMIT_PER_SECOND must be a valid u64".to_string(),
            })?;

        let smtp_host = std::env::var("CONMAN_SMTP_HOST").ok();
        let smtp_port: u16 = std::env::var("CONMAN_SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse()
            .map_err(|_| ConmanError::Validation {
                message: "CONMAN_SMTP_PORT must be a valid u16".to_string(),
            })?;
        let smtp_username = std::env::var("CONMAN_SMTP_USERNAME").ok();
        let smtp_password = std::env::var("CONMAN_SMTP_PASSWORD").ok();
        let smtp_from_email = std::env::var("CONMAN_SMTP_FROM_EMAIL").ok();
        let msuite_submit_cmd =
            std::env::var("CONMAN_MSUITE_SUBMIT_CMD").unwrap_or_else(|_| "true".to_string());
        let msuite_merge_cmd =
            std::env::var("CONMAN_MSUITE_MERGE_CMD").unwrap_or_else(|_| "true".to_string());
        let msuite_deploy_cmd =
            std::env::var("CONMAN_MSUITE_DEPLOY_CMD").unwrap_or_else(|_| "true".to_string());
        let deploy_release_cmd =
            std::env::var("CONMAN_DEPLOY_RELEASE_CMD").unwrap_or_else(|_| "true".to_string());
        let runtime_profile_drift_check_cmd =
            std::env::var("CONMAN_RUNTIME_PROFILE_DRIFT_CHECK_CMD")
                .unwrap_or_else(|_| "true".to_string());
        let temp_env_provision_cmd = std::env::var("CONMAN_TEMP_ENV_PROVISION_CMD").ok();
        let temp_env_expire_cmd = std::env::var("CONMAN_TEMP_ENV_EXPIRE_CMD").ok();

        if smtp_host.is_some() && smtp_from_email.is_none() {
            return Err(ConmanError::Validation {
                message: "CONMAN_SMTP_FROM_EMAIL is required when CONMAN_SMTP_HOST is set"
                    .to_string(),
            });
        }
        if smtp_username.is_some() ^ smtp_password.is_some() {
            return Err(ConmanError::Validation {
                message: "CONMAN_SMTP_USERNAME and CONMAN_SMTP_PASSWORD must be provided together"
                    .to_string(),
            });
        }

        Ok(Self {
            listen_addr,
            mongo_uri: std::env::var("CONMAN_MONGO_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".to_string()),
            mongo_db: std::env::var("CONMAN_MONGO_DB").unwrap_or_else(|_| "conman".to_string()),
            gitaly_address: std::env::var("CONMAN_GITALY_ADDRESS")
                .unwrap_or_else(|_| "http://localhost:8075".to_string()),
            jwt_secret,
            jwt_expiry_hours,
            invite_expiry_days,
            secrets_master_key,
            temp_url_domain,
            http_rate_limit_per_second,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            smtp_from_email,
            msuite_submit_cmd,
            msuite_merge_cmd,
            msuite_deploy_cmd,
            deploy_release_cmd,
            runtime_profile_drift_check_cmd,
            temp_env_provision_cmd,
            temp_env_expire_cmd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn clear_env() {
        for key in [
            "CONMAN_HOST",
            "CONMAN_PORT",
            "CONMAN_MONGO_URI",
            "CONMAN_MONGO_DB",
            "CONMAN_GITALY_ADDRESS",
            "CONMAN_JWT_SECRET",
            "CONMAN_JWT_EXPIRY_HOURS",
            "CONMAN_INVITE_EXPIRY_DAYS",
            "CONMAN_SECRETS_MASTER_KEY",
            "CONMAN_TEMP_URL_DOMAIN",
            "CONMAN_HTTP_RATE_LIMIT_PER_SECOND",
            "CONMAN_SMTP_HOST",
            "CONMAN_SMTP_PORT",
            "CONMAN_SMTP_USERNAME",
            "CONMAN_SMTP_PASSWORD",
            "CONMAN_SMTP_FROM_EMAIL",
            "CONMAN_MSUITE_SUBMIT_CMD",
            "CONMAN_MSUITE_MERGE_CMD",
            "CONMAN_MSUITE_DEPLOY_CMD",
            "CONMAN_DEPLOY_RELEASE_CMD",
            "CONMAN_RUNTIME_PROFILE_DRIFT_CHECK_CMD",
            "CONMAN_TEMP_ENV_PROVISION_CMD",
            "CONMAN_TEMP_ENV_EXPIRE_CMD",
        ] {
            unsafe { std::env::remove_var(key) };
        }
    }

    #[test]
    fn config_loads_defaults() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("CONMAN_JWT_SECRET", "test-secret-test-secret-test-1234");
            std::env::set_var("CONMAN_SECRETS_MASTER_KEY", "master-key");
            std::env::set_var("CONMAN_TEMP_URL_DOMAIN", "example.test");
        }

        let config = Config::from_env().expect("config should load");

        assert_eq!(config.listen_addr.port(), 3000);
        assert_eq!(config.mongo_uri, "mongodb://localhost:27017");
        assert_eq!(config.mongo_db, "conman");
        assert_eq!(config.gitaly_address, "http://localhost:8075");
        assert_eq!(config.jwt_expiry_hours, 24);
        assert_eq!(config.invite_expiry_days, 7);
        assert_eq!(config.http_rate_limit_per_second, 200);
        assert!(config.smtp_host.is_none());
        assert_eq!(config.msuite_submit_cmd, "true");
        assert_eq!(config.msuite_merge_cmd, "true");
        assert_eq!(config.msuite_deploy_cmd, "true");
        assert_eq!(config.deploy_release_cmd, "true");
        assert_eq!(config.runtime_profile_drift_check_cmd, "true");
        assert!(config.temp_env_provision_cmd.is_none());
        assert!(config.temp_env_expire_cmd.is_none());
    }

    #[test]
    fn config_fails_without_jwt_secret() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("CONMAN_SECRETS_MASTER_KEY", "master-key");
            std::env::set_var("CONMAN_TEMP_URL_DOMAIN", "example.test");
        }

        let result = Config::from_env();

        assert!(result.is_err());
        assert!(
            result
                .expect_err("must fail")
                .to_string()
                .contains("CONMAN_JWT_SECRET")
        );
    }

    #[test]
    fn config_fails_on_invalid_port() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("CONMAN_JWT_SECRET", "test-secret-test-secret-test-1234");
            std::env::set_var("CONMAN_PORT", "bad-port");
            std::env::set_var("CONMAN_SECRETS_MASTER_KEY", "master-key");
            std::env::set_var("CONMAN_TEMP_URL_DOMAIN", "example.test");
        }

        let result = Config::from_env();

        assert!(result.is_err());
        assert!(
            result
                .expect_err("must fail")
                .to_string()
                .contains("CONMAN_PORT")
        );
    }
}
