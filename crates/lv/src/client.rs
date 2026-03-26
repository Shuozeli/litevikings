use anyhow::Result;
use tonic::transport::Channel;

use lv_proto::litevikings::v1::admin_service_client::AdminServiceClient;
use lv_proto::litevikings::v1::filesystem_service_client::FilesystemServiceClient;
use lv_proto::litevikings::v1::resources_service_client::ResourcesServiceClient;
use lv_proto::litevikings::v1::search_service_client::SearchServiceClient;
use lv_proto::litevikings::v1::sessions_service_client::SessionsServiceClient;
use lv_proto::litevikings::v1::*;

/// Typed gRPC client wrapper over generated tonic stubs.
pub struct LvClient {
    fs: FilesystemServiceClient<Channel>,
    resources: ResourcesServiceClient<Channel>,
    sessions: SessionsServiceClient<Channel>,
    search: SearchServiceClient<Channel>,
    admin: AdminServiceClient<Channel>,
    api_key: Option<String>,
}

impl LvClient {
    pub async fn connect(addr: &str, api_key: Option<String>) -> Result<Self> {
        let channel = Channel::from_shared(addr.to_string())?.connect().await?;
        Ok(Self {
            fs: FilesystemServiceClient::new(channel.clone()),
            resources: ResourcesServiceClient::new(channel.clone()),
            sessions: SessionsServiceClient::new(channel.clone()),
            search: SearchServiceClient::new(channel.clone()),
            admin: AdminServiceClient::new(channel),
            api_key,
        })
    }

    fn request<T>(&self, inner: T) -> tonic::Request<T> {
        let mut req = tonic::Request::new(inner);
        if let Some(key) = &self.api_key {
            if let Ok(val) = key.parse() {
                req.metadata_mut().insert("x-api-key", val);
            }
        }
        req
    }

    // --- Filesystem ---

    pub async fn ls(&mut self, uri: &str, recursive: bool, limit: i32) -> Result<Vec<DirEntry>> {
        let resp = self
            .fs
            .ls(self.request(LsRequest {
                uri: uri.to_string(),
                simple: false,
                recursive,
                output: "agent".to_string(),
                abs_limit: 256,
                show_all_hidden: false,
                node_limit: limit,
            }))
            .await?;
        Ok(resp.into_inner().entries)
    }

    pub async fn mkdir(&mut self, uri: &str) -> Result<()> {
        self.fs
            .mkdir(self.request(MkdirRequest {
                uri: uri.to_string(),
            }))
            .await?;
        Ok(())
    }

    pub async fn rm(&mut self, uri: &str, recursive: bool) -> Result<()> {
        self.fs
            .rm(self.request(RmRequest {
                uri: uri.to_string(),
                recursive,
            }))
            .await?;
        Ok(())
    }

    pub async fn read(&mut self, uri: &str) -> Result<String> {
        let resp = self
            .fs
            .read(self.request(ReadRequest {
                uri: uri.to_string(),
            }))
            .await?;
        Ok(resp.into_inner().content)
    }

    pub async fn read_abstract(&mut self, uri: &str) -> Result<String> {
        let resp = self
            .fs
            .read_abstract(self.request(ReadAbstractRequest {
                uri: uri.to_string(),
            }))
            .await?;
        Ok(resp.into_inner().abstract_text)
    }

    pub async fn write(&mut self, uri: &str, content: &str) -> Result<()> {
        self.fs
            .write(self.request(WriteRequest {
                uri: uri.to_string(),
                content: content.to_string(),
            }))
            .await?;
        Ok(())
    }

    // --- Search ---

    pub async fn find(
        &mut self,
        query: &str,
        target_uri: Option<&str>,
        limit: i32,
    ) -> Result<FindResponse> {
        let resp = self
            .search
            .find(self.request(FindRequest {
                query: query.to_string(),
                target_uri: target_uri.map(String::from),
                limit,
                node_limit: None,
                score_threshold: None,
                filter: None,
            }))
            .await?;
        Ok(resp.into_inner())
    }

    // --- Resources ---

    pub async fn add_resource(
        &mut self,
        source: &str,
        target_uri: Option<&str>,
        wait: bool,
    ) -> Result<AddResourceResponse> {
        let resp = self
            .resources
            .add_resource(self.request(AddResourceRequest {
                path: Some(source.to_string()),
                temp_path: None,
                to: target_uri.map(String::from),
                parent: None,
                reason: String::new(),
                instruction: String::new(),
                wait,
                timeout: None,
                strict: true,
                ignore_dirs: None,
                include: None,
                exclude: None,
                directly_upload_media: true,
                preserve_structure: None,
                watch_interval: 0.0,
            }))
            .await?;
        Ok(resp.into_inner())
    }

    #[allow(dead_code)]
    pub async fn wait_processed(&mut self) -> Result<()> {
        self.resources
            .wait_processed(self.request(WaitProcessedRequest { timeout: None }))
            .await?;
        Ok(())
    }

    // --- Sessions ---

    pub async fn session_create(&mut self) -> Result<CreateSessionResponse> {
        let resp = self
            .sessions
            .create(self.request(CreateSessionRequest { session_id: None }))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn session_get(&mut self, id: &str) -> Result<GetSessionResponse> {
        let resp = self
            .sessions
            .get(self.request(GetSessionRequest {
                session_id: id.to_string(),
            }))
            .await?;
        Ok(resp.into_inner())
    }

    pub async fn session_delete(&mut self, id: &str) -> Result<()> {
        self.sessions
            .delete(self.request(DeleteSessionRequest {
                session_id: id.to_string(),
            }))
            .await?;
        Ok(())
    }

    pub async fn session_list(&mut self) -> Result<Vec<GetSessionResponse>> {
        let resp = self
            .sessions
            .list(self.request(ListSessionsRequest {}))
            .await?;
        Ok(resp.into_inner().sessions)
    }

    pub async fn session_add_message(&mut self, id: &str, role: &str, text: &str) -> Result<()> {
        self.sessions
            .add_message(self.request(AddMessageRequest {
                session_id: id.to_string(),
                role: role.to_string(),
                content: Some(text.to_string()),
                parts: vec![],
            }))
            .await?;
        Ok(())
    }

    pub async fn session_get_messages(&mut self, id: &str) -> Result<Vec<Message>> {
        let resp = self
            .sessions
            .get_messages(self.request(GetMessagesRequest {
                session_id: id.to_string(),
            }))
            .await?;
        Ok(resp.into_inner().messages)
    }

    pub async fn session_commit(&mut self, id: &str) -> Result<CommitResponse> {
        let resp = self
            .sessions
            .commit(self.request(CommitRequest {
                session_id: id.to_string(),
            }))
            .await?;
        Ok(resp.into_inner())
    }

    // --- Admin ---

    pub async fn status(&mut self) -> Result<SystemStatusResponse> {
        let resp = self.admin.status(self.request(StatusRequest {})).await?;
        Ok(resp.into_inner())
    }
}
