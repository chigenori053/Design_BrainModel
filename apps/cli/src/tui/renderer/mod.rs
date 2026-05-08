use std::io::{self, Stdout};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::runtime::event_queue::RuntimeEventQueue;
use crate::runtime::logging::{
    assert_alternate_screen_exclusive_output, enter_tui_surface, leave_tui_surface,
};
use crate::tui::{render, rendering::RenderSnapshot};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderGenerationIds {
    pub frame_generation_id: u64,
    pub repaint_request_id: u64,
    pub repaint_generation_id: u64,
    pub present_completed_generation: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FramePresentationLifecycle {
    ids: RenderGenerationIds,
}

impl FramePresentationLifecycle {
    pub fn begin_frame(&mut self) -> u64 {
        self.ids.frame_generation_id = self.ids.frame_generation_id.saturating_add(1);
        self.ids.frame_generation_id
    }

    pub fn begin_render(&mut self, request_id: u64) -> Result<u64, String> {
        if request_id < self.ids.repaint_request_id {
            return Err(format!(
                "Stale repaint request: {request_id} < {}",
                self.ids.repaint_request_id
            ));
        }
        self.ids.repaint_request_id = request_id;
        self.ids.repaint_generation_id = self.ids.repaint_generation_id.saturating_add(1);
        Ok(self.ids.repaint_generation_id)
    }

    pub fn present_surface(&mut self, repaint_generation_id: u64) -> Result<(), String> {
        if repaint_generation_id < self.ids.present_completed_generation {
            return Err(format!(
                "Out-of-order present: {repaint_generation_id} < {}",
                self.ids.present_completed_generation
            ));
        }
        self.ids.present_completed_generation = repaint_generation_id;
        Ok(())
    }

    pub fn ids(&self) -> RenderGenerationIds {
        self.ids
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderScheduler {
    repaint_requested: bool,
    sequence: u64,
    dropped_stale_repaint_count: u64,
    state_generation_id: u64,
    repaint_requested_generation: u64,
    repaint_completed_generation: u64,
}

impl RenderScheduler {
    pub fn observe_queue(&mut self, queue: &RuntimeEventQueue) {
        if !queue.is_empty() {
            self.request_full_repaint();
        }
    }

    pub fn request(&mut self) {
        self.request_full_repaint();
    }

    pub fn request_full_repaint(&mut self) {
        if self.repaint_requested {
            // If the state hasn't changed, we can coalesce.
            // But if it's the same generation, we still increment dropped count.
            self.dropped_stale_repaint_count = self.dropped_stale_repaint_count.saturating_add(1);
        }
        self.repaint_requested = true;
        self.repaint_requested_generation = self.state_generation_id;
    }

    pub fn notify_state_change(&mut self) {
        self.state_generation_id = self.state_generation_id.saturating_add(1);
        self.request_full_repaint();
    }

    pub fn take_pending(&mut self) -> Option<u64> {
        if self.repaint_requested {
            self.repaint_requested = false;
            self.sequence = self.sequence.saturating_add(1);
            Some(self.sequence)
        } else {
            None
        }
    }

    pub fn on_repaint_complete(&mut self, repaint_generation_id: u64) {
        self.repaint_completed_generation = repaint_generation_id;
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn dropped_stale_repaint_count(&self) -> u64 {
        self.dropped_stale_repaint_count
    }

    pub fn pending_repaint_queue_depth(&self) -> usize {
        if self.repaint_requested { 1 } else { 0 }
    }

    pub fn state_generation_id(&self) -> u64 {
        self.state_generation_id
    }

    pub fn repaint_requested_generation(&self) -> u64 {
        self.repaint_requested_generation
    }

    pub fn repaint_completed_generation(&self) -> u64 {
        self.repaint_completed_generation
    }
}

pub struct TerminalRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
    lifecycle: FramePresentationLifecycle,
}

impl TerminalRenderer {
    pub fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|err| err.to_string())?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            Hide,
            Clear(ClearType::All),
            MoveTo(0, 0)
        ) {
            disable_raw_mode().ok();
            return Err(err.to_string());
        }
        enter_tui_surface();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(err) => {
                leave_tui_surface();
                return Err(err.to_string());
            }
        };
        if let Err(err) = terminal.clear() {
            leave_tui_surface();
            return Err(err.to_string());
        }
        Ok(Self {
            terminal,
            active: true,
            lifecycle: FramePresentationLifecycle::default(),
        })
    }

    pub fn full_repaint(
        &mut self,
        request_id: u64,
        snapshot: &RenderSnapshot,
    ) -> Result<(), String> {
        assert_alternate_screen_exclusive_output()?;
        self.lifecycle.begin_frame();
        self.terminal.clear().map_err(|err| err.to_string())?;
        let repaint_generation_id = self.lifecycle.begin_render(request_id)?;
        self.terminal
            .draw(|frame| render::render(frame, snapshot))
            .map_err(|err| err.to_string())?;
        self.lifecycle.present_surface(repaint_generation_id)?;
        Ok(())
    }

    pub fn generation_ids(&self) -> RenderGenerationIds {
        self.lifecycle.ids()
    }

    pub fn shutdown(mut self) {
        self.restore();
    }

    fn restore(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        disable_raw_mode().ok();
        execute!(
            self.terminal.backend_mut(),
            Show,
            Clear(ClearType::All),
            MoveTo(0, 0),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .ok();
        self.terminal.show_cursor().ok();
        leave_tui_surface();
    }
}

impl Drop for TerminalRenderer {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::logging::{
        assert_alternate_screen_exclusive_output, enter_tui_surface, leave_tui_surface,
        record_stderr_write, record_stdout_write, reset_terminal_write_detection,
        stderr_write_detected, stdout_write_detected, terminal_output_test_lock,
        tui_surface_active,
    };

    #[test]
    fn scheduler_only_tracks_full_repaint_requests() {
        let mut scheduler = RenderScheduler::default();
        assert!(scheduler.take_pending().is_none());

        scheduler.request();
        assert_eq!(scheduler.take_pending(), Some(1));
        assert_eq!(scheduler.sequence(), 1);
        assert!(scheduler.take_pending().is_none());

        scheduler.request_full_repaint();
        scheduler.request_full_repaint();
        assert_eq!(scheduler.dropped_stale_repaint_count(), 1);
        assert_eq!(scheduler.take_pending(), Some(2));
        assert_eq!(scheduler.sequence(), 2);
    }

    #[test]
    fn frame_generation_monotonic() {
        let mut lifecycle = FramePresentationLifecycle::default();

        let first_frame = lifecycle.begin_frame();
        let first_render = lifecycle.begin_render(1).unwrap();
        lifecycle.present_surface(first_render).unwrap();
        let first = lifecycle.ids();

        let second_frame = lifecycle.begin_frame();
        let second_render = lifecycle.begin_render(2).unwrap();
        lifecycle.present_surface(second_render).unwrap();
        let second = lifecycle.ids();

        assert_eq!(first_frame, 1);
        assert_eq!(first_render, 1);
        assert_eq!(first.present_completed_generation, 1);
        assert_eq!(second_frame, 2);
        assert_eq!(second_render, 2);
        assert_eq!(second.present_completed_generation, 2);
        assert!(second.frame_generation_id > first.frame_generation_id);
        assert!(second.repaint_generation_id > first.repaint_generation_id);
        assert!(second.present_completed_generation > first.present_completed_generation);
    }

    #[test]
    fn stale_repaint_never_presented() {
        let mut lifecycle = FramePresentationLifecycle::default();
        lifecycle.begin_render(10).unwrap();
        lifecycle.present_surface(1).unwrap();

        // Attempting to render an older request should fail
        let result = lifecycle.begin_render(5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale repaint request"));

        // Attempting to present an older generation should fail
        let result = lifecycle.present_surface(0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Out-of-order present"));
    }

    #[test]
    fn latest_frame_wins() {
        let mut scheduler = RenderScheduler::default();
        scheduler.request_full_repaint();
        scheduler.request_full_repaint();
        scheduler.request_full_repaint();

        assert_eq!(scheduler.dropped_stale_repaint_count(), 2);
        let request_id = scheduler.take_pending().unwrap();
        assert_eq!(request_id, 1); // Sequence starts at 1
    }

    #[test]
    fn pending_queue_discards_old_frames() {
        let mut scheduler = RenderScheduler::default();
        assert_eq!(scheduler.pending_repaint_queue_depth(), 0);

        scheduler.request_full_repaint();
        assert_eq!(scheduler.pending_repaint_queue_depth(), 1);

        scheduler.request_full_repaint();
        assert_eq!(scheduler.pending_repaint_queue_depth(), 1);
        assert_eq!(scheduler.dropped_stale_repaint_count(), 1);

        scheduler.take_pending();
        assert_eq!(scheduler.pending_repaint_queue_depth(), 0);
    }

    #[test]
    fn no_stdout_write_during_tui() {
        let _lock = terminal_output_test_lock();
        enter_tui_surface();
        reset_terminal_write_detection();

        assert!(tui_surface_active());
        assert!(!stdout_write_detected());
        assert_alternate_screen_exclusive_output().expect("exclusive output");

        leave_tui_surface();
    }

    #[test]
    fn no_stderr_write_during_tui() {
        let _lock = terminal_output_test_lock();
        enter_tui_surface();
        reset_terminal_write_detection();

        assert!(tui_surface_active());
        assert!(!stderr_write_detected());
        assert_alternate_screen_exclusive_output().expect("exclusive output");

        leave_tui_surface();
    }

    #[test]
    fn state_transition_requires_completed_repaint() {
        let mut scheduler = RenderScheduler::default();
        scheduler.notify_state_change();
        assert_eq!(scheduler.state_generation_id(), 1);
        assert_eq!(scheduler.repaint_requested_generation(), 1);
        assert_eq!(scheduler.repaint_completed_generation(), 0);

        let request_id = scheduler.take_pending().unwrap();
        scheduler.on_repaint_complete(request_id);
        assert_eq!(scheduler.repaint_completed_generation(), 1);
    }

    #[test]
    fn repaint_coalescing_never_drops_latest_state() {
        let mut scheduler = RenderScheduler::default();
        scheduler.notify_state_change(); // gen 1
        scheduler.notify_state_change(); // gen 2

        assert_eq!(scheduler.state_generation_id(), 2);
        assert_eq!(scheduler.repaint_requested_generation(), 2);

        let request_id = scheduler.take_pending().unwrap();
        assert_eq!(request_id, 1);
        // Even if we coalesced, the latest request_id (1) corresponds to the latest state (gen 2)
    }

    #[test]
    fn every_state_generation_reaches_surface() {
        let mut lifecycle = FramePresentationLifecycle::default();

        // Gen 1
        let repaint_gen_1 = lifecycle.begin_render(1).unwrap();
        lifecycle.present_surface(repaint_gen_1).unwrap();
        assert_eq!(lifecycle.ids().present_completed_generation, 1);

        // Gen 2
        let repaint_gen_2 = lifecycle.begin_render(2).unwrap();
        lifecycle.present_surface(repaint_gen_2).unwrap();
        assert_eq!(lifecycle.ids().present_completed_generation, 2);
    }

    #[test]
    fn applying_frame_cannot_survive_newer_presented_generation() {
        let mut lifecycle = FramePresentationLifecycle::default();
        let _gen1 = lifecycle.begin_render(1).unwrap();
        let gen2 = lifecycle.begin_render(2).unwrap();
        lifecycle.present_surface(gen2).unwrap();

        // Attempting to present gen 1 after gen 2 should fail
        let result = lifecycle.present_surface(1);
        assert!(result.is_err());
    }

    #[test]
    fn preview_ready_always_presented() {
        let mut scheduler = RenderScheduler::default();
        scheduler.notify_state_change(); // Assume this is for PreviewReady

        assert!(scheduler.take_pending().is_some());
    }

    #[test]
    fn alternate_screen_has_exclusive_output() {
        let _lock = terminal_output_test_lock();
        enter_tui_surface();
        reset_terminal_write_detection();
        assert!(assert_alternate_screen_exclusive_output().is_ok());

        record_stdout_write();
        assert!(assert_alternate_screen_exclusive_output().is_err());

        reset_terminal_write_detection();
        record_stderr_write();
        assert!(assert_alternate_screen_exclusive_output().is_err());

        leave_tui_surface();
        reset_terminal_write_detection();
    }
}
