use crate::{
    dto::sqs_event::GitSource,
    error::{ACError, ACResult},
    git_local::GitLocal,
    github::GitHub,
    timeouts::Timeouts,
};
use async_trait::async_trait;
use std::{path::Path, time::SystemTime};

#[derive(Clone)]
pub enum GitRemote {
    GitHub(GitHub),
}

#[async_trait]
pub trait GitHost {
    fn provider(&self) -> &'static str;

    async fn account_repo_ids(&self) -> ACResult<Vec<String>>;

    async fn account_repo_name(&self, repo_id: &str) -> ACResult<(String, String)>;

    async fn clone_url(&self, repo_id: &str) -> ACResult<String>;
}

impl TryFrom<&GitSource> for GitRemote {
    type Error = ACError;

    fn try_from(value: &GitSource) -> Result<Self, Self::Error> {
        match value {
            GitSource::GitHub {
                installation_id, ..
            } => Ok(GitRemote::GitHub(GitHub::new(*installation_id)?)),
        }
    }
}

impl GitRemote {
    fn host(&self) -> &dyn GitHost {
        match self {
            GitRemote::GitHub(host) => host,
        }
    }

    pub fn provider(&self) -> &'static str {
        self.host().provider()
    }

    pub async fn account_repo_ids(&self) -> ACResult<Vec<String>> {
        self.host().account_repo_ids().await
    }

    pub async fn account_repo_name(&self, repo_id: &str) -> ACResult<(String, String)> {
        self.host().account_repo_name(repo_id).await
    }

    pub async fn clone_bare_repo(
        &self,
        timeouts: Timeouts,
        repo_id: &str,
        repo_dir: &str,
        deadline: SystemTime,
    ) -> ACResult<()> {
        let clone_url = self.host().clone_url(repo_id).await?;
        GitLocal::exec_git(
            timeouts,
            Path::new(repo_dir),
            &["clone", "--bare", &clone_url, repo_dir],
            deadline,
        )
        .await?;
        Ok(())
    }

    pub async fn set_remote(
        &self,
        timeouts: Timeouts,
        repo_id: &str,
        repo_dir: &Path,
        deadline: SystemTime,
    ) -> ACResult<()> {
        let clone_url = self.host().clone_url(repo_id).await?;
        GitLocal
            .set_remote(timeouts, repo_dir, &clone_url, deadline)
            .await?;
        Ok(())
    }
}
