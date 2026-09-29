use crate::{dto::sqs_event::SqsRefType, error::ACResult, program::Program, timeouts::Timeouts};
use std::{path::Path, time::SystemTime};

const STOCK_TREK_REF_PREFIX: &str = "refs/stock-trek";
const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;

pub struct GitLocal;

impl GitLocal {
    pub fn stock_trek_ref_name(ref_name: &str, commit_hash: &str) -> String {
        format!("{STOCK_TREK_REF_PREFIX}/{ref_name}-{commit_hash}")
    }

    pub async fn fetch(
        &self,
        timeouts: Timeouts,
        path: &Path,
        deadline: SystemTime,
    ) -> ACResult<String> {
        Self::exec_git(
            timeouts,
            path,
            &["fetch", "--prune", "--tags", "origin"],
            deadline,
        )
        .await
    }

    pub async fn set_remote(
        &self,
        timeouts: Timeouts,
        path: &Path,
        remote_url: &str,
        deadline: SystemTime,
    ) -> ACResult<String> {
        Self::exec_git(
            timeouts,
            path,
            &["remote", "set-url", "origin", remote_url],
            deadline,
        )
        .await
    }

    pub async fn create_ref(
        &self,
        timeouts: Timeouts,
        ref_name: &str,
        commit_hash: &str,
        path: &Path,
        deadline: SystemTime,
    ) -> ACResult<String> {
        Self::exec_git(
            timeouts,
            path,
            &["update-ref", "--", ref_name, commit_hash],
            deadline,
        )
        .await
    }

    pub async fn add_ref(
        &self,
        timeouts: Timeouts,
        path: &Path,
        ref_name: &str,
        ref_type: SqsRefType,
        deadline: SystemTime,
    ) -> ACResult<String> {
        let full_name = Self::full_ref_name(ref_name, ref_type);
        let refspec = format!("+{full_name}:{full_name}");
        Self::exec_git(timeouts, path, &["fetch", "origin", &refspec], deadline).await?;
        let commit_hash = Self::exec_git(
            timeouts,
            path,
            &["rev-parse", &format!("{full_name}^{{commit}}")],
            deadline,
        )
        .await?;
        let commit_hash = commit_hash.trim();
        let stock_trek_ref = Self::stock_trek_ref_name(ref_name, commit_hash);
        Self::exec_git(
            timeouts,
            path,
            &["update-ref", "--", &stock_trek_ref, commit_hash],
            deadline,
        )
        .await
    }

    pub async fn delete_ref(
        &self,
        timeouts: Timeouts,
        path: &Path,
        ref_name: &str,
        ref_type: SqsRefType,
        deadline: SystemTime,
    ) -> ACResult<String> {
        let prefix = format!("{STOCK_TREK_REF_PREFIX}/{ref_name}-");
        let stock_trek_refs = Self::exec_git(
            timeouts,
            path,
            &["for-each-ref", "--format=%(refname)", STOCK_TREK_REF_PREFIX],
            deadline,
        )
        .await?;
        for stock_trek_ref in stock_trek_refs
            .lines()
            .filter(|stock_trek_ref| Self::is_stock_trek_ref(stock_trek_ref, &prefix))
        {
            Self::exec_git(
                timeouts,
                path,
                &["update-ref", "-d", "--", stock_trek_ref],
                deadline,
            )
            .await?;
        }
        let full_name = Self::full_ref_name(ref_name, ref_type);
        Self::exec_git(
            timeouts,
            path,
            &["update-ref", "-d", "--", &full_name],
            deadline,
        )
        .await
    }

    pub async fn exec_git(
        timeouts: Timeouts,
        path: &Path,
        args: &[&str],
        deadline: SystemTime,
    ) -> ACResult<String> {
        let timeout = timeouts.command_for(deadline)?;
        Program::run_with_timeout("git", args, path, timeout).await
    }
}

// helpers
impl GitLocal {
    fn is_stock_trek_ref(stock_trek_ref: &str, prefix: &str) -> bool {
        stock_trek_ref.strip_prefix(prefix).is_some_and(|hash| {
            matches!(hash.len(), SHA1_HEX_LENGTH | SHA256_HEX_LENGTH)
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    }

    fn full_ref_name(ref_name: &str, ref_type: SqsRefType) -> String {
        let prefix = match ref_type {
            SqsRefType::Branch => "refs/heads",
            SqsRefType::Tag => "refs/tags",
        };
        format!("{prefix}/{ref_name}")
    }
}
