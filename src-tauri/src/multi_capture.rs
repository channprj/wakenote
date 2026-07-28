use std::collections::BTreeMap;

use crate::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, LiveCaptureError, LiveCaptureRuntime,
};
use crate::settings::MicrophoneSlot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrophoneRuntimeDiagnostic {
    pub slot: MicrophoneSlot,
    pub running: bool,
    pub dropped_frame_count: u64,
    pub runtime_error: Option<String>,
}

pub struct MultiCaptureRuntime<B: AudioInputBackend + Default> {
    runtimes: BTreeMap<MicrophoneSlot, LiveCaptureRuntime<B>>,
}

impl<B: AudioInputBackend + Default> Default for MultiCaptureRuntime<B> {
    fn default() -> Self {
        Self {
            runtimes: BTreeMap::new(),
        }
    }
}

impl<B: AudioInputBackend + Default> MultiCaptureRuntime<B> {
    pub fn start_slot(
        &mut self,
        slot: MicrophoneSlot,
        config: AudioInputConfig,
        on_frame: impl Fn(AudioFrame) + Send + Sync + 'static,
    ) -> Result<(), LiveCaptureError> {
        if let Some(runtime) = self.runtimes.get_mut(&slot) {
            return runtime.start(config, on_frame);
        }

        let mut runtime = LiveCaptureRuntime::new(B::default());
        runtime.start(config, on_frame)?;
        self.runtimes.insert(slot, runtime);
        Ok(())
    }

    pub fn stop_slot(&mut self, slot: MicrophoneSlot) {
        if let Some(mut runtime) = self.runtimes.remove(&slot) {
            runtime.stop();
        }
    }

    pub fn stop_all(&mut self) {
        for runtime in self.runtimes.values_mut() {
            runtime.stop();
        }
        self.runtimes.clear();
    }

    pub fn is_running(&self, slot: MicrophoneSlot) -> bool {
        self.runtimes
            .get(&slot)
            .is_some_and(LiveCaptureRuntime::is_running)
    }

    pub fn active_slots(&self) -> Vec<MicrophoneSlot> {
        self.runtimes
            .iter()
            .filter_map(|(slot, runtime)| runtime.is_running().then_some(*slot))
            .collect()
    }

    pub fn diagnostics(&self) -> Vec<MicrophoneRuntimeDiagnostic> {
        self.runtimes
            .iter()
            .map(|(slot, runtime)| MicrophoneRuntimeDiagnostic {
                slot: *slot,
                running: runtime.is_running(),
                dropped_frame_count: runtime.dropped_frame_count(),
                runtime_error: runtime.runtime_error(),
            })
            .collect()
    }

    pub fn diagnostic(&self, slot: MicrophoneSlot) -> Option<MicrophoneRuntimeDiagnostic> {
        self.runtimes
            .get(&slot)
            .map(|runtime| MicrophoneRuntimeDiagnostic {
                slot,
                running: runtime.is_running(),
                dropped_frame_count: runtime.dropped_frame_count(),
                runtime_error: runtime.runtime_error(),
            })
    }

    pub fn total_dropped_frame_count(&self) -> u64 {
        self.runtimes
            .values()
            .map(LiveCaptureRuntime::dropped_frame_count)
            .sum()
    }

    pub fn first_runtime_error(&self) -> Option<String> {
        self.runtimes
            .values()
            .find_map(LiveCaptureRuntime::runtime_error)
    }
}
