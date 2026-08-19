use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
#[cfg(test)]
use crate::session::turn_context::TurnContext;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
#[cfg(test)]
use crate::tools::handlers::ToolSearchHandlerCache;
use crate::tools::registry::AnyToolResult;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolArgumentDiffConsumer;
use crate::tools::registry::ToolRegistry;
#[cfg(test)]
use crate::tools::spec_plan::finalize_tool_router;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::SearchToolCallParams;
use codex_tools::DiscoverableTool;
use codex_tools::LoadableToolSpec;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

pub use crate::tools::context::ToolCallSource;

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub tool_name: ToolName,
    pub call_id: String,
    pub payload: ToolPayload,
    pub encrypted_function_args: Option<Vec<String>>,
}

impl ToolCall {
    pub(crate) fn direct_source(&self) -> ToolCallSource {
        if self.tool_name.namespace.as_deref() == Some("collaboration")
            && matches!(
                self.tool_name.name.as_str(),
                "spawn_agent" | "send_message" | "followup_task"
            )
            && self
                .encrypted_function_args
                .as_ref()
                .is_some_and(Vec::is_empty)
        {
            ToolCallSource::DirectPlaintextMessage
        } else {
            ToolCallSource::Direct
        }
    }
}

pub(crate) fn tool_log_payload<'a>(
    payload: &'a ToolPayload,
    source: &ToolCallSource,
) -> Cow<'a, str> {
    if matches!(source, ToolCallSource::DirectPlaintextMessage) {
        return Cow::Borrowed("[plaintext arguments]");
    }
    payload.log_payload()
}

pub struct ToolRouter {
    registry: ToolRegistry,
    model_visible_specs: Vec<ToolSpec>,
    loaded_deferred_tools: Arc<LoadedDeferredToolSet>,
}

#[derive(Debug, Default)]
pub(crate) struct LoadedDeferredToolSet {
    tools: Mutex<BTreeMap<ToolName, LoadedDeferredTool>>,
}

#[derive(Clone, Debug)]
enum LoadedDeferredTool {
    Function(ResponsesApiTool),
    NamespaceTool {
        namespace: String,
        namespace_description: String,
        tool: ResponsesApiNamespaceTool,
    },
}

impl LoadedDeferredTool {
    fn from_loadable(spec: LoadableToolSpec) -> Vec<(ToolName, Self)> {
        match spec {
            LoadableToolSpec::Function(tool) => {
                let name = ToolName::plain(tool.name.clone()).with_default_namespace();
                vec![(name, Self::Function(tool))]
            }
            LoadableToolSpec::Namespace(namespace) => namespace
                .tools
                .into_iter()
                .map(|tool| {
                    let tool_name = match &tool {
                        ResponsesApiNamespaceTool::Function(tool) => tool.name.clone(),
                        ResponsesApiNamespaceTool::Custom(tool) => tool.name.clone(),
                    };
                    let name = ToolName::namespaced(namespace.name.clone(), tool_name);
                    (
                        name,
                        Self::NamespaceTool {
                            namespace: namespace.name.clone(),
                            namespace_description: namespace.description.clone(),
                            tool,
                        },
                    )
                })
                .collect(),
        }
    }

    fn has_same_schema(&self, candidate: &Self) -> bool {
        match (self, candidate) {
            (Self::Function(existing), Self::Function(candidate)) => {
                existing.strict == candidate.strict
                    && existing.parameters == candidate.parameters
                    && existing.output_schema == candidate.output_schema
            }
            (
                Self::NamespaceTool { tool: existing, .. },
                Self::NamespaceTool {
                    tool: candidate, ..
                },
            ) => match (existing, candidate) {
                (
                    ResponsesApiNamespaceTool::Function(existing),
                    ResponsesApiNamespaceTool::Function(candidate),
                ) => {
                    existing.strict == candidate.strict
                        && existing.parameters == candidate.parameters
                        && existing.output_schema == candidate.output_schema
                }
                (
                    ResponsesApiNamespaceTool::Custom(existing),
                    ResponsesApiNamespaceTool::Custom(candidate),
                ) => existing.format == candidate.format,
                (ResponsesApiNamespaceTool::Function(_), ResponsesApiNamespaceTool::Custom(_))
                | (ResponsesApiNamespaceTool::Custom(_), ResponsesApiNamespaceTool::Function(_)) => {
                    false
                }
            },
            (Self::Function(_), Self::NamespaceTool { .. })
            | (Self::NamespaceTool { .. }, Self::Function(_)) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolSuggestPresentation {
    ListTool,
    RecommendationContext,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolSuggestCandidates {
    pub(crate) tools: Vec<DiscoverableTool>,
    pub(crate) presentation: ToolSuggestPresentation,
}

impl ToolRouter {
    #[cfg(test)]
    pub(crate) fn from_registry(
        turn_context: &TurnContext,
        registry: ToolRegistry,
        hosted_specs: Vec<ToolSpec>,
        tool_search_handler_cache: &ToolSearchHandlerCache,
    ) -> Self {
        finalize_tool_router(
            turn_context,
            registry,
            hosted_specs,
            tool_search_handler_cache,
        )
        .expect("test tool registry should not contain duplicate tools")
    }

    #[cfg(test)]
    pub(crate) fn from_parts(registry: ToolRegistry, model_visible_specs: Vec<ToolSpec>) -> Self {
        Self::from_parts_with_loaded_deferred_tools(
            registry,
            model_visible_specs,
            Arc::new(LoadedDeferredToolSet::default()),
        )
    }

    pub(crate) fn from_parts_with_loaded_deferred_tools(
        registry: ToolRegistry,
        model_visible_specs: Vec<ToolSpec>,
        loaded_deferred_tools: Arc<LoadedDeferredToolSet>,
    ) -> Self {
        Self {
            registry,
            model_visible_specs,
            loaded_deferred_tools,
        }
    }

    pub(crate) fn model_visible_specs(&self) -> Vec<ToolSpec> {
        let mut specs = self.model_visible_specs.clone();
        let mut namespace_indices = specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| match spec {
                ToolSpec::Namespace(namespace) => Some((namespace.name.clone(), index)),
                ToolSpec::Function(_)
                | ToolSpec::Freeform(_)
                | ToolSpec::ToolSearch { .. }
                | ToolSpec::WebSearch { .. } => None,
            })
            .collect::<BTreeMap<_, _>>();
        let loaded = self.loaded_deferred_tools.tools.lock();
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(poisoned) => poisoned.into_inner(),
        };

        for loaded_tool in loaded.values() {
            match loaded_tool {
                LoadedDeferredTool::Function(tool) => specs.push(ToolSpec::Function(tool.clone())),
                LoadedDeferredTool::NamespaceTool {
                    namespace,
                    namespace_description,
                    tool,
                } => {
                    if let Some(index) = namespace_indices.get(namespace).copied() {
                        let ToolSpec::Namespace(existing) = &mut specs[index] else {
                            unreachable!("namespace index must point to a namespace spec");
                        };
                        existing.tools.push(tool.clone());
                    } else {
                        namespace_indices.insert(namespace.clone(), specs.len());
                        specs.push(ToolSpec::Namespace(ResponsesApiNamespace {
                            name: namespace.clone(),
                            description: namespace_description.clone(),
                            tools: vec![tool.clone()],
                        }));
                    }
                }
            }
        }

        specs
    }

    pub(crate) fn register_loaded_deferred_tools(
        &self,
        specs: &[LoadableToolSpec],
    ) -> Result<(), FunctionCallError> {
        let loaded = self.loaded_deferred_tools.tools.lock();
        let mut loaded = match loaded {
            Ok(loaded) => loaded,
            Err(poisoned) => poisoned.into_inner(),
        };

        for (name, candidate) in specs
            .iter()
            .cloned()
            .flat_map(LoadedDeferredTool::from_loadable)
        {
            if let Some(existing) = loaded.get(&name) {
                if existing.has_same_schema(&candidate) {
                    continue;
                }
                return Err(FunctionCallError::Fatal(format!(
                    "tool_search returned conflicting schemas for deferred tool `{name}`"
                )));
            }
            loaded.insert(name, candidate);
        }

        Ok(())
    }

    pub(crate) fn deferred_tool_namespaces(&self) -> BTreeMap<String, String> {
        self.registry.deferred_tool_namespaces()
    }

    #[cfg(test)]
    pub(crate) fn registered_tool_names_for_test(&self) -> Vec<ToolName> {
        self.registry.tool_names_for_test()
    }

    #[cfg(test)]
    pub(crate) fn tool_exposure_for_test(
        &self,
        name: &ToolName,
    ) -> Option<crate::tools::registry::ToolExposure> {
        self.registry.tool_exposure(name)
    }

    pub(crate) fn create_diff_consumer(
        &self,
        tool_name: &ToolName,
    ) -> Option<Box<dyn ToolArgumentDiffConsumer>> {
        self.registry.create_diff_consumer(tool_name)
    }

    pub fn tool_supports_parallel(&self, call: &ToolCall) -> bool {
        self.registry
            .supports_parallel_tool_calls(&call.tool_name)
            .unwrap_or(false)
    }

    pub(crate) fn tool_runtime(&self, call: &ToolCall) -> Option<Arc<dyn CoreToolRuntime>> {
        self.registry.tool(&call.tool_name)
    }

    pub fn tool_waits_for_runtime_cancellation(&self, call: &ToolCall) -> bool {
        self.registry
            .waits_for_runtime_cancellation(&call.tool_name)
            .unwrap_or(false)
    }

    #[instrument(level = "trace", skip_all, err)]
    pub fn build_tool_call(item: ResponseItem) -> Result<Option<ToolCall>, FunctionCallError> {
        match item {
            ResponseItem::FunctionCall {
                name,
                namespace,
                arguments,
                encrypted_function_args,
                call_id,
                ..
            } => {
                let tool_name = ToolName::new(namespace, name).with_default_namespace();
                Ok(Some(ToolCall {
                    tool_name,
                    call_id,
                    payload: ToolPayload::Function { arguments },
                    encrypted_function_args,
                }))
            }
            ResponseItem::ToolSearchCall {
                call_id: Some(call_id),
                execution,
                arguments,
                ..
            } if execution == "client" => {
                let arguments: SearchToolCallParams =
                    serde_json::from_value(arguments).map_err(|err| {
                        FunctionCallError::RespondToModel(format!(
                            "failed to parse tool_search arguments: {err}"
                        ))
                    })?;
                Ok(Some(ToolCall {
                    tool_name: ToolName::plain("tool_search"),
                    call_id,
                    payload: ToolPayload::ToolSearch { arguments },
                    encrypted_function_args: None,
                }))
            }
            ResponseItem::ToolSearchCall { .. } => Ok(None),
            ResponseItem::CustomToolCall {
                name,
                namespace,
                input,
                call_id,
                ..
            } => Ok(Some(ToolCall {
                tool_name: ToolName::new(namespace, name).with_default_namespace(),
                call_id,
                payload: ToolPayload::Custom { input },
                encrypted_function_args: None,
            })),
            _ => Ok(None),
        }
    }

    #[allow(dead_code)]
    #[instrument(level = "trace", skip_all, err)]
    pub async fn dispatch_tool_call_with_code_mode_result(
        &self,
        session: Arc<Session>,
        step_context: Arc<StepContext>,
        cancellation_token: CancellationToken,
        tracker: SharedTurnDiffTracker,
        call: ToolCall,
        source: ToolCallSource,
    ) -> Result<AnyToolResult, FunctionCallError> {
        self.dispatch_tool_call_with_code_mode_result_inner(
            session,
            step_context,
            cancellation_token,
            tracker,
            call,
            source,
            /*terminal_outcome_reached*/ None,
        )
        .await
    }

    #[instrument(level = "trace", skip_all, err)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_tool_call_with_terminal_outcome(
        &self,
        session: Arc<Session>,
        step_context: Arc<StepContext>,
        cancellation_token: CancellationToken,
        tracker: SharedTurnDiffTracker,
        call: ToolCall,
        source: ToolCallSource,
        terminal_outcome_reached: Arc<AtomicBool>,
    ) -> Result<AnyToolResult, FunctionCallError> {
        self.dispatch_tool_call_with_code_mode_result_inner(
            session,
            step_context,
            cancellation_token,
            tracker,
            call,
            source,
            Some(terminal_outcome_reached),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch_tool_call_with_code_mode_result_inner(
        &self,
        session: Arc<Session>,
        step_context: Arc<StepContext>,
        cancellation_token: CancellationToken,
        tracker: SharedTurnDiffTracker,
        call: ToolCall,
        source: ToolCallSource,
        terminal_outcome_reached: Option<Arc<AtomicBool>>,
    ) -> Result<AnyToolResult, FunctionCallError> {
        let ToolCall {
            tool_name,
            call_id,
            payload,
            ..
        } = call;

        // Keep the legacy ToolInvocation.turn field tied to the same request state until handlers migrate.
        let turn = Arc::clone(&step_context.turn);
        let invocation = ToolInvocation {
            session,
            turn,
            step_context,
            cancellation_token,
            tracker,
            call_id,
            tool_name,
            source,
            payload,
        };

        self.registry
            .dispatch_any_with_terminal_outcome(invocation, terminal_outcome_reached)
            .await
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
