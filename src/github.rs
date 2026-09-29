use crate::{
    error::{ACError, ACResult},
    git_remote::GitHost,
};
use async_trait::async_trait;
use jsonwebtoken::EncodingKey;
use octocrab::{Octocrab, models::InstallationId};
use serde::{Deserialize, Serialize};

const GITHUB_APP_ID_ENV: &str = "GITHUB_APP_ID";
const GITHUB_APP_PRIVATE_KEY_ENV: &str = "GITHUB_APP_PRIVATE_KEY";
const GITHUB_PER_PAGE: u32 = 100;
const GITHUB_PROVIDER: &str = "github";

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepo {
    id: u64,
    name: String,
    clone_url: String,
    owner: GitHubOwner,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubOwner {
    login: String,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<GitHubRepo>,
}

#[derive(Debug, Serialize)]
struct RepositoriesParams {
    per_page: u32,
    page: u32,
}

#[derive(Clone)]
pub struct GitHub {
    client: Octocrab,
}

#[async_trait]
impl GitHost for GitHub {
    fn provider(&self) -> &'static str {
        GITHUB_PROVIDER
    }

    async fn account_repo_ids(&self) -> ACResult<Vec<String>> {
        Ok(self
            .repos()
            .await?
            .into_iter()
            .map(|repo| repo.id.to_string())
            .collect())
    }

    async fn account_repo_name(&self, repo_id: &str) -> ACResult<(String, String)> {
        let repo = self.repo(Self::parse_repo_id(repo_id)?).await?;
        Ok((repo.owner.login, repo.name))
    }

    async fn clone_url(&self, repo_id: &str) -> ACResult<String> {
        Ok(self.repo(Self::parse_repo_id(repo_id)?).await?.clone_url)
    }
}

impl GitHub {
    pub fn new(installation_id: u64) -> ACResult<Self> {
        let app_id = Self::required(GITHUB_APP_ID_ENV)?;
        let app_id: u64 = app_id
            .parse()
            .map_err(|error| ACError::Config(format!("{GITHUB_APP_ID_ENV} is invalid: {error}")))?;
        let private_key = Self::required(GITHUB_APP_PRIVATE_KEY_ENV)?.replace("\\n", "\n");
        let key = EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|error| {
            ACError::Config(format!("{GITHUB_APP_PRIVATE_KEY_ENV} is invalid: {error}"))
        })?;
        let client = Octocrab::builder()
            .app(app_id.into(), key)
            .build()
            .map_err(|error| ACError::GitHub(format!("failed to build GitHub client: {error}")))?
            .installation(InstallationId::from(installation_id))
            .map_err(|error| {
                ACError::GitHub(format!(
                    "failed to create client for installation {installation_id}: {error}"
                ))
            })?;
        Ok(Self { client })
    }
}

// helpers
impl GitHub {
    async fn repos(&self) -> ACResult<Vec<GitHubRepo>> {
        let mut repos = Vec::new();
        let mut page = 1;
        loop {
            let params = RepositoriesParams {
                per_page: GITHUB_PER_PAGE,
                page,
            };
            let response = self
                .client
                .get::<InstallationRepositoriesResponse, _, _>(
                    "/installation/repositories",
                    Some(&params),
                )
                .await
                .map_err(|error| {
                    ACError::GitHub(format!("failed to list installation repositories: {error}"))
                })?;
            let received = response.repositories.len();
            repos.extend(response.repositories);
            if received < GITHUB_PER_PAGE as usize {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }

    async fn repo(&self, repo_id: u64) -> ACResult<GitHubRepo> {
        self.client
            .get::<GitHubRepo, _, _>(format!("/repositories/{repo_id}"), None::<&()>)
            .await
            .map_err(|error| {
                ACError::GitHub(format!("failed to get repository {repo_id}: {error}"))
            })
    }

    fn parse_repo_id(repo_id: &str) -> ACResult<u64> {
        repo_id
            .parse()
            .map_err(|_| ACError::InvalidMessage(format!("Invalid repo id: {repo_id:?}")))
    }

    fn required(key: &str) -> ACResult<String> {
        match std::env::var(key) {
            Ok(value) if !value.is_empty() => Ok(value),
            Ok(_) => Err(ACError::Config(format!("{key} must not be empty"))),
            Err(std::env::VarError::NotPresent) => {
                Err(ACError::Config(format!("{key} must be set")))
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(ACError::Config(format!("{key} is not valid UTF-8")))
            }
        }
    }
}
