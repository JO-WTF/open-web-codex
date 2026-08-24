use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use codex_exec_server::ExecProcess;
use codex_exec_server::ExecProcessEventReceiver;
use codex_exec_server::ExecProcessFuture;
use codex_exec_server::ProcessId;
use codex_exec_server::ProcessSignal;
use codex_exec_server::ReadResponse;
use codex_exec_server::WriteResponse;
use pretty_assertions::assert_eq;
use tokio::sync::watch;

use super::ExecutorProcessTransport;
use super::StdioServerProcessHandle;
use super::StdioServerTransport;
use super::StdioServerTransportInner;

struct CountingExecProcess {
    process_id: ProcessId,
    terminate_calls: AtomicUsize,
}

impl ExecProcess for CountingExecProcess {
    fn process_id(&self) -> &ProcessId {
        &self.process_id
    }

    fn subscribe_wake(&self) -> watch::Receiver<u64> {
        watch::channel(0).1
    }

    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        ExecProcessEventReceiver::empty()
    }

    fn read(
        &self,
        _after_seq: Option<u64>,
        _max_bytes: Option<usize>,
        _wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        Box::pin(async { unreachable!("termination test should not read process output") })
    }

    fn write(&self, _chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        Box::pin(async { unreachable!("termination test should not write process input") })
    }

    fn signal(&self, _signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        self.terminate_calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn stdio_process_handles_share_a_terminal_state() {
    let process = StdioServerProcessHandle::local("test".to_string(), None);
    let observer = process.clone();

    process.mark_closed();

    assert!(observer.is_closed());
}

#[tokio::test]
async fn executor_stdio_transport_terminates_once_across_close_and_drop() {
    let process = Arc::new(CountingExecProcess {
        process_id: ProcessId::from("mcp-stdio-liveness"),
        terminate_calls: AtomicUsize::new(0),
    });
    let process_handle =
        StdioServerProcessHandle::executor("mcp-stdio-liveness".to_string(), process.clone());
    let mut transport = StdioServerTransport {
        inner: StdioServerTransportInner::Executor(ExecutorProcessTransport::new(
            process.clone(),
            "mcp-stdio-liveness".to_string(),
        )),
        process: process_handle.clone(),
    };

    rmcp::transport::Transport::close(&mut transport)
        .await
        .expect("stdio transport close should terminate the process");
    drop(transport);
    drop(process_handle);

    assert_eq!(process.terminate_calls.load(Ordering::Acquire), 1);
}
