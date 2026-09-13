//! Bucket versioning and public read access.
//!
//! "Public" means anonymous `s3:GetObject` on every object. The app manages it with a
//! policy statement of its own (`LiteExplorerPublicRead`) and leaves other statements
//! alone. On AWS the bucket's Block Public Access settings are relaxed for policies
//! while public and restored when private. Cloudflare R2 supports neither feature
//! through the S3 API, and other services may answer `NotImplemented`.

use aws_sdk_s3::{
    error::{ProvideErrorMetadata, SdkError},
    types::{BucketVersioningStatus, PublicAccessBlockConfiguration, VersioningConfiguration},
    Client,
};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use super::{
    client_for, describe_error, load_connection, parse_remote_path, Provider, RemoteClients,
};
use crate::Database;

const PUBLIC_READ_SID: &str = "LiteExplorerPublicRead";

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Versioning {
    /// Never enabled.
    Off,
    Enabled,
    Suspended,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PublicAccess {
    /// Anyone can read objects.
    public: bool,
    /// Public because of a policy statement the app didn't write.
    public_elsewhere: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BucketSettings {
    provider: Provider,
    /// `None` when the service doesn't support versioning.
    versioning: Option<Versioning>,
    /// `None` when public access can't be managed through the S3 API.
    public_access: Option<PublicAccess>,
}

fn error_code<E: ProvideErrorMetadata, R>(error: &SdkError<E, R>) -> Option<String> {
    match error {
        SdkError::ServiceError(context) => context.err().code().map(str::to_string),
        _ => None,
    }
}

fn is_unsupported<E: ProvideErrorMetadata, R>(error: &SdkError<E, R>) -> bool {
    matches!(
        error_code(error).as_deref(),
        Some("NotImplemented" | "XNotImplemented" | "MethodNotAllowed" | "UnsupportedOperation")
    )
}

fn settings_error<E, R>(error: SdkError<E, R>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
    R: std::fmt::Debug,
{
    match error_code(&error).as_deref() {
        Some("AccessDenied" | "Forbidden") => {
            "These credentials don’t have permission to change this bucket’s settings. An account-wide Block Public Access setting can also prevent public policies."
                .into()
        }
        _ => describe_error(error),
    }
}

fn as_array(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(item) => vec![item],
        None => Vec::new(),
    }
}

fn is_public_read(statement: &Value) -> bool {
    let allows = statement.get("Effect").and_then(Value::as_str) == Some("Allow");
    let anyone = match statement.get("Principal") {
        Some(Value::String(principal)) => principal == "*",
        Some(Value::Object(principal)) => as_array(principal.get("AWS"))
            .iter()
            .any(|value| value.as_str() == Some("*")),
        _ => false,
    };
    let reads = as_array(statement.get("Action"))
        .iter()
        .any(|action| matches!(action.as_str(), Some("s3:GetObject" | "s3:*" | "*")));
    // Conditions (IP ranges, referers, ...) make the grant narrower than "anyone".
    allows && anyone && reads && statement.get("Condition").is_none()
}

fn is_ours(statement: &Value) -> bool {
    statement.get("Sid").and_then(Value::as_str) == Some(PUBLIC_READ_SID)
}

/// Whether `policy` grants public read, and whether any such grant isn't the app's own.
fn policy_public_read(policy: &str) -> (bool, bool) {
    let Ok(document) = serde_json::from_str::<Value>(policy) else {
        return (false, false);
    };
    let grants: Vec<_> = as_array(document.get("Statement"))
        .into_iter()
        .filter(|statement| is_public_read(statement))
        .collect();
    (
        !grants.is_empty(),
        grants.iter().any(|statement| !is_ours(statement)),
    )
}

/// `policy` with the app's public-read statement added or removed. `None` means no
/// statements are left and the policy should be deleted.
fn policy_with_public_read(
    policy: Option<&str>,
    bucket: &str,
    public: bool,
) -> Result<Option<String>, String> {
    let mut document = match policy {
        Some(policy) => serde_json::from_str::<Value>(policy)
            .map_err(|error| format!("The bucket policy isn’t valid JSON: {error}"))?,
        None => json!({ "Version": "2012-10-17" }),
    };
    let mut statements: Vec<Value> = as_array(document.get("Statement"))
        .into_iter()
        .filter(|statement| !is_ours(statement))
        .cloned()
        .collect();
    if public {
        statements.push(json!({
            "Sid": PUBLIC_READ_SID,
            "Effect": "Allow",
            "Principal": "*",
            "Action": ["s3:GetObject"],
            "Resource": [format!("arn:aws:s3:::{bucket}/*")],
        }));
    }
    if statements.is_empty() {
        return Ok(None);
    }
    document["Statement"] = Value::Array(statements);
    Ok(Some(document.to_string()))
}

async fn read_versioning(client: &Client, bucket: &str) -> Result<Option<Versioning>, String> {
    match client.get_bucket_versioning().bucket(bucket).send().await {
        Ok(output) => Ok(Some(match output.status() {
            Some(BucketVersioningStatus::Enabled) => Versioning::Enabled,
            Some(BucketVersioningStatus::Suspended) => Versioning::Suspended,
            _ => Versioning::Off,
        })),
        Err(error) if is_unsupported(&error) => Ok(None),
        Err(error) => Err(settings_error(error)),
    }
}

/// `Ok(None)` when the bucket has no policy, `Err(None)` when policies aren't supported.
async fn read_policy(client: &Client, bucket: &str) -> Result<Option<String>, Option<String>> {
    match client.get_bucket_policy().bucket(bucket).send().await {
        Ok(output) => Ok(output.policy().map(str::to_string)),
        Err(error) if error_code(&error).as_deref() == Some("NoSuchBucketPolicy") => Ok(None),
        Err(error) if is_unsupported(&error) => Err(None),
        Err(error) => Err(Some(settings_error(error))),
    }
}

/// Whether Block Public Access stops public policies from taking effect on this bucket.
async fn policies_blocked(client: &Client, bucket: &str) -> Result<bool, String> {
    match client.get_public_access_block().bucket(bucket).send().await {
        Ok(output) => Ok(output
            .public_access_block_configuration()
            .and_then(|config| config.restrict_public_buckets())
            .unwrap_or(false)),
        Err(error)
            if is_unsupported(&error)
                || error_code(&error).as_deref()
                    == Some("NoSuchPublicAccessBlockConfiguration") =>
        {
            Ok(false)
        }
        Err(error) => Err(settings_error(error)),
    }
}

async fn read_public_access(client: &Client, bucket: &str) -> Result<Option<PublicAccess>, String> {
    let policy = match read_policy(client, bucket).await {
        Ok(policy) => policy,
        Err(None) => return Ok(None),
        Err(Some(error)) => return Err(error),
    };
    let (grants, elsewhere) = policy
        .as_deref()
        .map(policy_public_read)
        .unwrap_or((false, false));
    let blocked = grants && policies_blocked(client, bucket).await?;
    Ok(Some(PublicAccess {
        public: grants && !blocked,
        public_elsewhere: elsewhere && !blocked,
    }))
}

async fn read_settings(
    client: &Client,
    provider: Provider,
    bucket: &str,
) -> Result<BucketSettings, String> {
    if provider == Provider::R2 {
        return Ok(BucketSettings {
            provider,
            versioning: None,
            public_access: None,
        });
    }
    Ok(BucketSettings {
        provider,
        versioning: read_versioning(client, bucket).await?,
        public_access: read_public_access(client, bucket).await?,
    })
}

pub(super) async fn apply_versioning(
    client: &Client,
    bucket: &str,
    enabled: bool,
) -> Result<(), String> {
    let status = if enabled {
        BucketVersioningStatus::Enabled
    } else {
        BucketVersioningStatus::Suspended
    };
    client
        .put_bucket_versioning()
        .bucket(bucket)
        .versioning_configuration(VersioningConfiguration::builder().status(status).build())
        .send()
        .await
        .map_err(settings_error)?;
    Ok(())
}

async fn apply_public_access_block(
    client: &Client,
    bucket: &str,
    block_policies: bool,
) -> Result<(), String> {
    let config = PublicAccessBlockConfiguration::builder()
        .block_public_acls(true)
        .ignore_public_acls(true)
        .block_public_policy(block_policies)
        .restrict_public_buckets(block_policies)
        .build();
    match client
        .put_public_access_block()
        .bucket(bucket)
        .public_access_block_configuration(config)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if is_unsupported(&error) => Ok(()),
        Err(error) => Err(settings_error(error)),
    }
}

pub(super) async fn apply_public(
    client: &Client,
    provider: Provider,
    bucket: &str,
    public: bool,
) -> Result<(), String> {
    if provider == Provider::R2 {
        return Err("Public access for R2 buckets is managed in the Cloudflare dashboard.".into());
    }
    if public {
        apply_public_access_block(client, bucket, false).await?;
    }
    let policy = match read_policy(client, bucket).await {
        Ok(policy) => policy,
        Err(None) => return Err("This service doesn’t support bucket policies.".into()),
        Err(Some(error)) => return Err(error),
    };
    match policy_with_public_read(policy.as_deref(), bucket, public)? {
        Some(document) => {
            client
                .put_bucket_policy()
                .bucket(bucket)
                .policy(document)
                .send()
                .await
                .map_err(settings_error)?;
        }
        None if policy.is_some() => {
            client
                .delete_bucket_policy()
                .bucket(bucket)
                .send()
                .await
                .map_err(settings_error)?;
        }
        None => {}
    }
    if !public {
        apply_public_access_block(client, bucket, true).await?;
    }
    Ok(())
}

async fn bucket_client(
    database: &Database,
    clients: &RemoteClients,
    path: &str,
) -> Result<(Client, Provider, String), String> {
    let remote = parse_remote_path(path).ok_or("Not a remote path")?;
    let bucket = match (remote.bucket, remote.key.is_empty()) {
        (Some(bucket), true) => bucket,
        _ => return Err("Choose a bucket".into()),
    };
    let provider = load_connection(&database.0, &remote.id).await?.provider;
    let client = client_for(&database.0, clients, &remote.id).await?;
    Ok((client, provider, bucket))
}

#[tauri::command]
pub async fn bucket_settings(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    path: String,
) -> Result<BucketSettings, String> {
    let (client, provider, bucket) = bucket_client(&database, &clients, &path).await?;
    read_settings(&client, provider, &bucket).await
}

#[tauri::command]
pub async fn set_bucket_versioning(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    path: String,
    enabled: bool,
) -> Result<BucketSettings, String> {
    let (client, provider, bucket) = bucket_client(&database, &clients, &path).await?;
    if provider == Provider::R2 {
        return Err("R2 doesn’t support versioning.".into());
    }
    apply_versioning(&client, &bucket, enabled).await?;
    read_settings(&client, provider, &bucket).await
}

#[tauri::command]
pub async fn set_bucket_public(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    path: String,
    public: bool,
) -> Result<BucketSettings, String> {
    let (client, provider, bucket) = bucket_client(&database, &clients, &path).await?;
    apply_public(&client, provider, &bucket, public).await?;
    read_settings(&client, provider, &bucket).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_read_statement_is_added_and_removed_without_touching_others() {
        let existing = r#"{"Version":"2012-10-17","Statement":{"Sid":"Logs","Effect":"Allow","Principal":{"Service":"logging.s3.amazonaws.com"},"Action":"s3:PutObject","Resource":"arn:aws:s3:::media/logs/*"}}"#;

        let public = policy_with_public_read(Some(existing), "media", true)
            .unwrap()
            .unwrap();
        let document: Value = serde_json::from_str(&public).unwrap();
        assert_eq!(document["Statement"].as_array().unwrap().len(), 2);
        assert_eq!(policy_public_read(&public), (true, false));

        let again = policy_with_public_read(Some(&public), "media", true)
            .unwrap()
            .unwrap();
        let document: Value = serde_json::from_str(&again).unwrap();
        assert_eq!(document["Statement"].as_array().unwrap().len(), 2);

        let private = policy_with_public_read(Some(&public), "media", false)
            .unwrap()
            .unwrap();
        assert_eq!(policy_public_read(&private), (false, false));
        assert!(private.contains("\"Logs\""));
    }

    #[test]
    fn removing_the_only_statement_deletes_the_policy() {
        let public = policy_with_public_read(None, "media", true)
            .unwrap()
            .unwrap();
        assert!(public.contains("arn:aws:s3:::media/*"));
        assert_eq!(
            policy_with_public_read(Some(&public), "media", false).unwrap(),
            None
        );
        assert_eq!(policy_with_public_read(None, "media", false).unwrap(), None);
    }

    #[test]
    fn detects_public_grants_written_elsewhere() {
        let other = r#"{"Statement":[{"Effect":"Allow","Principal":{"AWS":["*"]},"Action":["s3:GetObject"],"Resource":"arn:aws:s3:::media/*"}]}"#;
        assert_eq!(policy_public_read(other), (true, true));

        let conditional = r#"{"Statement":[{"Effect":"Allow","Principal":"*","Action":"s3:GetObject","Resource":"arn:aws:s3:::media/*","Condition":{"IpAddress":{"aws:SourceIp":"10.0.0.0/8"}}}]}"#;
        assert_eq!(policy_public_read(conditional), (false, false));

        let deny = r#"{"Statement":[{"Effect":"Deny","Principal":"*","Action":"s3:*","Resource":"arn:aws:s3:::media/*"}]}"#;
        assert_eq!(policy_public_read(deny), (false, false));
        assert_eq!(policy_public_read("not json"), (false, false));
    }

    #[test]
    fn invalid_existing_policy_is_reported() {
        assert!(policy_with_public_read(Some("{"), "media", true).is_err());
    }
}
