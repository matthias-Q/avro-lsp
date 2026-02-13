use async_lsp::router::Router;
use async_lsp::{ClientSocket, LanguageClient, LanguageServer, ResponseError, lsp_types::*};
use futures::future::BoxFuture;
use std::ops::ControlFlow;

use crate::state::ServerState;

pub struct AvroLanguageServer {
    state: ServerState,
    client: ClientSocket,
}

impl AvroLanguageServer {
    pub fn new(client: ClientSocket) -> Self {
        Self {
            state: ServerState::new(),
            client,
        }
    }

    pub fn new_router(client: ClientSocket) -> Router<Self> {
        Router::from_language_server(Self::new(client))
    }
}

impl LanguageServer for AvroLanguageServer {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn initialize(
        &mut self,
        params: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, Self::Error>> {
        tracing::info!("Initializing avro-lsp server");
        tracing::debug!("Client capabilities: {:?}", params.capabilities);

        Box::pin(async move {
            Ok(InitializeResult {
                capabilities: ServerCapabilities {
                    text_document_sync: Some(TextDocumentSyncCapability::Options(
                        TextDocumentSyncOptions {
                            open_close: Some(true),
                            change: Some(TextDocumentSyncKind::FULL),
                            will_save: None,
                            will_save_wait_until: None,
                            save: None,
                        },
                    )),
                    hover_provider: Some(HoverProviderCapability::Simple(true)),
                    document_symbol_provider: Some(OneOf::Left(true)),
                    definition_provider: Some(OneOf::Left(true)),
                    completion_provider: Some(CompletionOptions {
                        trigger_characters: Some(vec![
                            "\"".to_string(),
                            ":".to_string(),
                            ",".to_string(),
                        ]),
                        resolve_provider: Some(false),
                        ..Default::default()
                    }),
                    semantic_tokens_provider: Some(
                        SemanticTokensServerCapabilities::SemanticTokensOptions(
                            SemanticTokensOptions {
                                legend: SemanticTokensLegend {
                                    token_types: vec![
                                        SemanticTokenType::KEYWORD,
                                        SemanticTokenType::TYPE,
                                        SemanticTokenType::ENUM,
                                        SemanticTokenType::STRUCT,
                                        SemanticTokenType::PROPERTY,
                                        SemanticTokenType::ENUM_MEMBER,
                                        SemanticTokenType::STRING,
                                        SemanticTokenType::NUMBER,
                                    ],
                                    token_modifiers: vec![
                                        SemanticTokenModifier::DECLARATION,
                                        SemanticTokenModifier::READONLY,
                                    ],
                                },
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                                range: None,
                                ..Default::default()
                            },
                        ),
                    ),
                    document_formatting_provider: Some(OneOf::Left(true)),
                    code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                    rename_provider: Some(OneOf::Right(RenameOptions {
                        prepare_provider: Some(true),
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    })),
                    references_provider: Some(OneOf::Left(true)),
                    ..Default::default()
                },
                server_info: Some(ServerInfo {
                    name: "avro-lsp".to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                }),
            })
        })
    }

    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> Self::NotifyResult {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        tracing::info!("Document opened: {}", uri);

        let state = self.state.clone();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            let diagnostics = state.did_open(uri.clone(), text, version).await;

            // Publish diagnostics
            let _ = client.publish_diagnostics(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: Some(version),
            });
        });

        ControlFlow::Continue(())
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> Self::NotifyResult {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // Get the full text from the first content change (FULL sync)
        if let Some(change) = params.content_changes.first() {
            tracing::debug!("Document changed: {}", uri);

            let state = self.state.clone();
            let mut client = self.client.clone();
            let text = change.text.clone();

            tokio::spawn(async move {
                let diagnostics = state.did_change(uri.clone(), text, version).await;

                // Publish diagnostics
                let _ = client.publish_diagnostics(PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: Some(version),
                });
            });
        }

        ControlFlow::Continue(())
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> Self::NotifyResult {
        let uri = params.text_document.uri;
        tracing::info!("Document closed: {}", uri);

        let state = self.state.clone();
        let mut client = self.client.clone();

        tokio::spawn(async move {
            state.did_close(&uri).await;

            // Clear diagnostics
            let _ = client.publish_diagnostics(PublishDiagnosticsParams {
                uri,
                diagnostics: vec![],
                version: None,
            });
        });

        ControlFlow::Continue(())
    }

    fn hover(
        &mut self,
        params: HoverParams,
    ) -> BoxFuture<'static, Result<Option<Hover>, Self::Error>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Hover request at {}:{}", uri, position.line);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_hover(&uri, position).await {
                Some(hover) => Ok(Some(hover)),
                None => Ok(None),
            }
        })
    }

    fn document_symbol(
        &mut self,
        params: DocumentSymbolParams,
    ) -> BoxFuture<'static, Result<Option<DocumentSymbolResponse>, Self::Error>> {
        let uri = params.text_document.uri;

        tracing::debug!("Document symbol request for {}", uri);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_document_symbols(&uri).await {
                Some(symbols) => Ok(Some(DocumentSymbolResponse::Nested(symbols))),
                None => Ok(None),
            }
        })
    }

    fn semantic_tokens_full(
        &mut self,
        params: SemanticTokensParams,
    ) -> BoxFuture<'static, Result<Option<SemanticTokensResult>, Self::Error>> {
        let uri = params.text_document.uri;

        tracing::debug!("Semantic tokens request for {}", uri);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_semantic_tokens(&uri).await {
                Some(data) => Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data,
                }))),
                None => Ok(None),
            }
        })
    }

    fn completion(
        &mut self,
        params: CompletionParams,
    ) -> BoxFuture<'static, Result<Option<CompletionResponse>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        tracing::debug!("Completion request at {}:{}", uri, position.line);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_completions(&uri, position).await {
                Some(items) => Ok(Some(CompletionResponse::Array(items))),
                None => Ok(None),
            }
        })
    }

    fn definition(
        &mut self,
        params: GotoDefinitionParams,
    ) -> BoxFuture<'static, Result<Option<GotoDefinitionResponse>, Self::Error>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Go to definition request at {}:{}", uri, position.line);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_definition(&uri, position).await {
                Some(location) => Ok(Some(GotoDefinitionResponse::Scalar(location))),
                None => Ok(None),
            }
        })
    }

    fn formatting(
        &mut self,
        params: DocumentFormattingParams,
    ) -> BoxFuture<'static, Result<Option<Vec<TextEdit>>, Self::Error>> {
        let uri = params.text_document.uri;

        tracing::debug!("Format document request for {}", uri);

        let state = self.state.clone();

        Box::pin(async move {
            match state.format_document(&uri).await {
                Ok(Some(edit)) => Ok(Some(vec![edit])),
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            }
        })
    }

    fn code_action(
        &mut self,
        params: CodeActionParams,
    ) -> BoxFuture<'static, Result<Option<CodeActionResponse>, Self::Error>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let diagnostics = params.context.diagnostics;

        tracing::debug!("Code action request for {} at {:?}", uri, range);

        let state = self.state.clone();

        Box::pin(async move {
            match state.get_code_actions(&uri, range, diagnostics).await {
                Some(actions) => {
                    // Convert Vec<CodeAction> to Vec<CodeActionOrCommand>
                    let response: CodeActionResponse = actions
                        .into_iter()
                        .map(CodeActionOrCommand::CodeAction)
                        .collect();
                    Ok(Some(response))
                }
                None => Ok(None),
            }
        })
    }

    fn rename(
        &mut self,
        params: RenameParams,
    ) -> BoxFuture<'static, Result<Option<WorkspaceEdit>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        tracing::debug!(
            "Rename request for {} at {:?} to '{}'",
            uri,
            position,
            new_name
        );

        let state = self.state.clone();

        Box::pin(async move { state.rename(&uri, position, &new_name).await })
    }

    fn prepare_rename(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> BoxFuture<'static, Result<Option<PrepareRenameResponse>, Self::Error>> {
        let uri = params.text_document.uri;
        let position = params.position;

        tracing::debug!("Prepare rename request for {} at {:?}", uri, position);

        let state = self.state.clone();

        Box::pin(async move { state.prepare_rename(&uri, position).await })
    }

    fn references(
        &mut self,
        params: ReferenceParams,
    ) -> BoxFuture<'static, Result<Option<Vec<Location>>, Self::Error>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        tracing::debug!("Find references request for {} at {:?}", uri, position);

        let state = self.state.clone();

        Box::pin(async move {
            state
                .find_references(&uri, position, include_declaration)
                .await
        })
    }
}
