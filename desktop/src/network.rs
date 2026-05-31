// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Desktop host-network backends for emulated Amiga network cards.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names
)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::time::{Duration, Instant};

use libslirp::{Context, Handler, PollEvents};
use mio::unix::UnixReady;
use mio::{Events, Poll, PollOpt, Ready, Token};
use rumiga_core::emulator::Emulator;

/// Host-network backend selected for a desktop emulator run.
pub struct DesktopNetworkBackend {
    slirp: Option<SlirpBackend>,
}

impl DesktopNetworkBackend {
    /// Create the configured desktop network backend.
    pub fn new(config: &rumiga_api::NetworkConfig) -> Self {
        match config.backend {
            rumiga_api::NetworkBackend::Disabled => Self { slirp: None },
            rumiga_api::NetworkBackend::Slirp => Self {
                slirp: Some(SlirpBackend::new()),
            },
        }
    }

    /// Apply host-link state to the emulated A2065 card.
    pub fn apply_link_state(&self, emulator: &Emulator) {
        emulator
            .memory
            .a2065
            .borrow_mut()
            .set_link_up(self.slirp.is_some());
    }

    /// Pump guest packets into the host backend and deliver host packets back to the guest.
    ///
    /// # Errors
    /// Returns an error if the backend poller fails.
    pub fn pump(&self, emulator: &Emulator) -> Result<(), String> {
        let Some(slirp) = self.slirp.as_ref() else {
            return Ok(());
        };

        while let Some(frame) = emulator.memory.a2065.borrow_mut().take_transmitted_frame() {
            slirp.input(&frame);
        }

        slirp.dispatch()?;

        for frame in slirp.drain_guest_frames() {
            emulator
                .memory
                .a2065
                .borrow_mut()
                .queue_receive_frame(frame);
        }

        Ok(())
    }
}

struct SlirpBackend {
    context: Context<Rc<RefCell<SlirpHandler>>>,
    handler: Rc<RefCell<SlirpHandler>>,
}

impl SlirpBackend {
    fn new() -> Self {
        let handler = Rc::new(RefCell::new(SlirpHandler::new()));
        let context = Context::new(
            true,
            true,
            Ipv4Addr::new(10, 0, 2, 0),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(10, 0, 2, 2),
            false,
            Ipv6Addr::UNSPECIFIED,
            0,
            Ipv6Addr::UNSPECIFIED,
            Some("rumiga".to_owned()),
            None,
            None,
            None,
            Ipv4Addr::new(10, 0, 2, 15),
            Ipv4Addr::new(10, 0, 2, 3),
            Ipv6Addr::UNSPECIFIED,
            Vec::new(),
            None,
            handler.clone(),
        );

        Self { context, handler }
    }

    fn input(&self, frame: &[u8]) {
        self.context.input(frame);
    }

    fn dispatch(&self) -> Result<(), String> {
        self.run_due_timers();
        self.poll_host_fds()?;
        self.run_due_timers();
        Ok(())
    }

    fn drain_guest_frames(&self) -> Vec<Vec<u8>> {
        self.handler.borrow_mut().drain_guest_frames()
    }

    fn run_due_timers(&self) {
        let now = Instant::now();
        let timers = self.handler.borrow().timers.clone();
        for timer in timers {
            let should_run = {
                let mut timer = timer.borrow_mut();
                if timer.cancelled {
                    false
                } else if timer.deadline.is_some_and(|deadline| deadline <= now) {
                    timer.deadline = None;
                    true
                } else {
                    false
                }
            };
            if should_run {
                let mut timer = timer.borrow_mut();
                (timer.func)();
            }
        }
        self.handler.borrow_mut().retain_active_timers();
    }

    fn poll_host_fds(&self) -> Result<(), String> {
        let poll = Poll::new().map_err(|e| format!("Failed to create SLIRP poller: {e}"))?;
        let mut events = Events::with_capacity(64);
        let mut registrations: Vec<PollRegistration> = Vec::new();
        let mut registration_error: Option<io::Error> = None;
        let mut timeout_ms = 0;

        self.context.pollfds_fill(&mut timeout_ms, |fd, requested| {
            let token = Token(registrations.len());
            let readiness = to_mio_ready(requested);
            let evented = mio::unix::EventedFd(&fd);
            match poll.register(&evented, token, readiness, PollOpt::level()) {
                Ok(()) => {
                    registrations.push(PollRegistration {
                        requested,
                        ready: PollEvents::empty(),
                    });
                    i32::try_from(token.0).unwrap_or(-1)
                }
                Err(e) => {
                    registration_error = Some(e);
                    -1
                }
            }
        });

        if let Some(error) = registration_error {
            return Err(format!("Failed to register SLIRP poll fd: {error}"));
        }

        poll.poll(&mut events, Some(Duration::from_millis(0)))
            .map_err(|e| format!("Failed to poll SLIRP fds: {e}"))?;
        for event in &events {
            let idx = event.token().0;
            if let Some(registration) = registrations.get_mut(idx) {
                registration.ready = from_mio_ready(event.readiness()) & registration.requested;
            }
        }

        self.context.pollfds_poll(false, |idx| {
            usize::try_from(idx)
                .ok()
                .and_then(|idx| registrations.get(idx))
                .map_or_else(PollEvents::empty, |registration| registration.ready)
        });

        Ok(())
    }
}

struct PollRegistration {
    requested: PollEvents,
    ready: PollEvents,
}

struct SlirpHandler {
    start: Instant,
    guest_frames: VecDeque<Vec<u8>>,
    timers: Vec<SharedTimer>,
}

impl SlirpHandler {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            guest_frames: VecDeque::new(),
            timers: Vec::new(),
        }
    }

    fn drain_guest_frames(&mut self) -> Vec<Vec<u8>> {
        self.guest_frames.drain(..).collect()
    }

    fn retain_active_timers(&mut self) {
        self.timers.retain(|timer| !timer.borrow().cancelled);
    }
}

impl Handler for SlirpHandler {
    type Timer = SharedTimer;

    fn clock_get_ns(&mut self) -> i64 {
        let elapsed = self.start.elapsed();
        i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX)
    }

    fn send_packet(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.guest_frames.push_back(buf.to_vec());
        Ok(buf.len())
    }

    fn register_poll_fd(&mut self, _fd: RawFd) {}

    fn unregister_poll_fd(&mut self, _fd: RawFd) {}

    fn guest_error(&mut self, msg: &str) {
        eprintln!("SLIRP guest error: {msg}");
    }

    fn notify(&mut self) {}

    fn timer_new(&mut self, func: Box<dyn FnMut()>) -> Box<Self::Timer> {
        let timer = Rc::new(RefCell::new(SlirpTimer {
            func,
            deadline: None,
            cancelled: false,
        }));
        self.timers.push(timer.clone());
        Box::new(timer)
    }

    fn timer_mod(&mut self, timer: &mut Box<Self::Timer>, expire_time: i64) {
        let delay = if expire_time <= 0 {
            Duration::ZERO
        } else {
            Duration::from_millis(u64::try_from(expire_time).unwrap_or(u64::MAX))
        };
        timer.borrow_mut().deadline = Instant::now().checked_add(delay);
    }

    fn timer_free(&mut self, timer: Box<Self::Timer>) {
        timer.borrow_mut().cancelled = true;
    }
}

type SharedTimer = Rc<RefCell<SlirpTimer>>;

struct SlirpTimer {
    func: Box<dyn FnMut()>,
    deadline: Option<Instant>,
    cancelled: bool,
}

fn to_mio_ready(events: PollEvents) -> Ready {
    let mut ready = UnixReady::from(Ready::empty());

    if events.has_in() {
        ready.insert(Ready::readable());
    }
    if events.has_out() {
        ready.insert(Ready::writable());
    }
    if events.has_hup() {
        ready.insert(UnixReady::hup());
    }
    if events.has_err() {
        ready.insert(UnixReady::error());
    }

    Ready::from(ready)
}

fn from_mio_ready(ready: Ready) -> PollEvents {
    let mut events = PollEvents::empty();
    let ready = UnixReady::from(ready);

    if ready.is_readable() {
        events |= PollEvents::poll_in();
    }
    if ready.is_writable() {
        events |= PollEvents::poll_out();
    }
    if ready.is_hup() {
        events |= PollEvents::poll_hup();
    }
    if ready.is_error() {
        events |= PollEvents::poll_err();
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_is_absent_when_disabled() {
        let backend = DesktopNetworkBackend::new(&rumiga_api::NetworkConfig::default());

        assert!(backend.slirp.is_none());
    }

    #[test]
    fn slirp_backend_accepts_empty_dispatch() {
        let backend = SlirpBackend::new();

        backend.dispatch().expect("empty dispatch should succeed");
        assert!(backend.drain_guest_frames().is_empty());
    }
}
