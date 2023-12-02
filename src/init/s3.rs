use crate::prelude::*;

use s3::{creds::Credentials, Bucket, Region};

pub fn init_s3_bucket(cfg: &Config) -> Result<Bucket> {
    let bucket = Bucket::new(
        &cfg.r2_bucket,
        Region::R2 {
            account_id: cfg.r2_account_id.clone(),
        },
        Credentials::new(
            Some(&cfg.r2_s3_access_key_id),
            Some(&cfg.r2_s3_secret_access_key),
            None,
            None,
            None,
        )?,
    )?
    .with_path_style();

    return Ok(bucket);
}
