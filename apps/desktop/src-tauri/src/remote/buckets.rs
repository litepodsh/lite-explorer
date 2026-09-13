//! Creating and deleting buckets from the bucket list of an account location.
//!
//! There is no way to ask S3 up front whether the credentials may create or delete
//! buckets, so the request is sent and access errors are explained afterwards.

use aws_sdk_s3::{
    error::{ProvideErrorMetadata, SdkError},
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use serde::Serialize;
use tauri::State;

use super::{
    bucket_settings::{apply_public, apply_versioning},
    client_for, describe_error, load_connection, parse_remote_path, remote_path, Provider,
    RemoteClients,
};
use crate::{Database, DirectoryEntry};

/// Bucket naming rules shared by AWS and most S3-compatible services. R2 also
/// disallows dots.
fn validate_bucket_name(name: &str, provider: Provider) -> Result<(), String> {
    let rules = if provider == Provider::R2 {
        "Use 3-63 lowercase letters, numbers and hyphens, starting and ending with a letter or number."
    } else {
        "Use 3-63 lowercase letters, numbers, hyphens and dots, starting and ending with a letter or number."
    };
    let bytes = name.as_bytes();
    let allowed = |c: &u8| {
        c.is_ascii_lowercase()
            || c.is_ascii_digit()
            || *c == b'-'
            || (*c == b'.' && provider != Provider::R2)
    };
    let edges_ok = |c: Option<&u8>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let looks_like_ip =
        name.split('.').count() == 4 && name.split('.').all(|part| part.parse::<u8>().is_ok());
    if !(3..=63).contains(&bytes.len())
        || !bytes.iter().all(allowed)
        || !edges_ok(bytes.first())
        || !edges_ok(bytes.last())
        || name.contains("..")
        || looks_like_ip
        || name.starts_with("xn--")
        || name.ends_with("-s3alias")
    {
        return Err(rules.into());
    }
    Ok(())
}

/// AWS needs a location constraint outside us-east-1; R2 (`auto`) and the default region don't.
fn location_constraint(region: &str) -> Option<&str> {
    (region != "us-east-1" && region != "auto" && !region.is_empty()).then_some(region)
}

fn bucket_access_error<E, R>(action: &str, name: &str, error: SdkError<E, R>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
    R: std::fmt::Debug,
{
    let code = match &error {
        SdkError::ServiceError(context) => context.err().code().map(str::to_string),
        _ => None,
    };
    match code.as_deref() {
        Some("AccessDenied") | Some("Forbidden") => {
            format!("These credentials don’t have permission to {action}.")
        }
        Some("BucketAlreadyExists") => {
            format!("The name “{name}” is already taken. Bucket names are shared by all users of the service.")
        }
        Some("BucketAlreadyOwnedByYou") => format!("You already have a bucket named “{name}”."),
        Some("BucketNotEmpty") => format!("“{name}” isn’t empty. Delete its contents first."),
        Some("InvalidBucketName") => format!("“{name}” isn’t a valid bucket name."),
        Some("TooManyBuckets") => "This account has reached its bucket limit.".into(),
        _ => describe_error(error),
    }
}

#[derive(Serialize)]
pub struct CreatedBucket {
    bucket: DirectoryEntry,
    /// Set when the bucket exists but a requested setting couldn't be applied.
    warning: Option<String>,
}

/// Which provider a remote path belongs to, so the UI can hide unsupported options.
#[tauri::command]
pub async fn remote_provider(
    database: State<'_, Database>,
    path: String,
) -> Result<Provider, String> {
    let remote = parse_remote_path(&path).ok_or("Not a remote path")?;
    Ok(load_connection(&database.0, &remote.id).await?.provider)
}

#[tauri::command]
pub async fn create_remote_bucket(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    location: String,
    name: String,
    versioning: Option<bool>,
    public: Option<bool>,
) -> Result<CreatedBucket, String> {
    let remote = parse_remote_path(&location).ok_or("Not a remote path")?;
    if remote.bucket.is_some() {
        return Err("Buckets can only be created from the bucket list".into());
    }
    let name = name.trim().to_string();
    let connection = load_connection(&database.0, &remote.id).await?;
    validate_bucket_name(&name, connection.provider)?;
    let client = client_for(&database.0, &clients, &remote.id).await?;

    let mut request = client.create_bucket().bucket(&name);
    if let Some(region) = location_constraint(&connection.region) {
        request = request.create_bucket_configuration(
            CreateBucketConfiguration::builder()
                .location_constraint(BucketLocationConstraint::from(region))
                .build(),
        );
    }
    request
        .send()
        .await
        .map_err(|error| bucket_access_error("create buckets", &name, error))?;

    let mut failed = Vec::new();
    if versioning == Some(true) {
        if let Err(error) = apply_versioning(&client, &name, true).await {
            failed.push(format!("versioning: {error}"));
        }
    }
    if public == Some(true) {
        if let Err(error) = apply_public(&client, connection.provider, &name, true).await {
            failed.push(format!("public access: {error}"));
        }
    }
    let warning = (!failed.is_empty()).then(|| {
        format!(
            "“{name}” was created, but some settings couldn’t be applied. {}",
            failed.join(" ")
        )
    });

    Ok(CreatedBucket {
        bucket: DirectoryEntry {
            path: remote_path(&remote.id, Some(&name), None),
            name,
            is_directory: true,
            is_hidden: false,
            size: None,
            created: None,
            kind: Some("bucket"),
        },
        warning,
    })
}

#[tauri::command]
pub async fn delete_remote_bucket(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    path: String,
) -> Result<(), String> {
    let remote = parse_remote_path(&path).ok_or("Not a remote path")?;
    let bucket = match (remote.bucket, remote.key.is_empty()) {
        (Some(bucket), true) => bucket,
        _ => return Err("Only whole buckets can be deleted here".into()),
    };
    let client = client_for(&database.0, &clients, &remote.id).await?;
    let listing = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(1)
        .send()
        .await
        .map_err(|error| bucket_access_error("delete buckets", &bucket, error))?;
    if !listing.contents().is_empty() {
        return Err(format!(
            "“{bucket}” isn’t empty. Delete its contents first."
        ));
    }
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .map_err(|error| bucket_access_error("delete buckets", &bucket, error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_names_follow_s3_rules() {
        for valid in ["media", "my-bucket-2026", "logs.example.com", "a1b"] {
            assert!(
                validate_bucket_name(valid, Provider::Aws).is_ok(),
                "{valid}"
            );
        }
        for invalid in [
            "ab",
            "Media",
            "-media",
            "media-",
            "my..bucket",
            "my_bucket",
            "192.168.1.10",
            "xn--media",
            "media-s3alias",
            &"a".repeat(64),
        ] {
            assert!(
                validate_bucket_name(invalid, Provider::Aws).is_err(),
                "{invalid}"
            );
        }
        assert!(validate_bucket_name("logs.example", Provider::R2).is_err());
        assert!(validate_bucket_name("logs-example", Provider::R2).is_ok());
    }

    #[test]
    fn location_constraint_only_outside_default_regions() {
        assert_eq!(location_constraint("us-east-1"), None);
        assert_eq!(location_constraint("auto"), None);
        assert_eq!(location_constraint("eu-west-1"), Some("eu-west-1"));
    }
}
