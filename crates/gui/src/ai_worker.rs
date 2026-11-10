//! A bounded, thread-confined player. Only owned requests and replies cross threads.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, mpsc};

use crate::ai::{AiPlayerDecision, AiReply, AiRequest};

type DecisionResult = Result<AiPlayerDecision, String>;

/// Dropping closes delivery and the request queue, never joins an active search.
pub(crate) struct Worker {
    sender: Option<mpsc::SyncSender<AiRequest>>,
    // Serialize close with the short completion send: an atomic check alone
    // would permit a reply to be sent after close returned. Never held in search.
    open: Arc<Mutex<bool>>,
}

impl Worker {
    /// The factory, not its non-Send result, crosses the thread boundary.
    /// Completion sinks must be short and nonblocking (the native sink is a proxy).
    pub(crate) fn spawn<F, D, S>(factory: F, sink: S) -> std::io::Result<Self>
    where
        F: FnOnce() -> Result<D, String> + Send + 'static,
        D: FnMut(&AiRequest) -> DecisionResult + 'static,
        S: Fn(AiReply) -> bool + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel::<AiRequest>(1);
        let open = Arc::new(Mutex::new(true));
        let delivery = Arc::clone(&open);
        std::thread::Builder::new()
            .name("gomoku-ai".into())
            .spawn(move || {
                let mut decider = catch_unwind(AssertUnwindSafe(factory))
                    .unwrap_or_else(|_| Err("player initialization panicked".into()));
                while let Ok(request) = receiver.recv() {
                    if !*delivery.lock().unwrap_or_else(|e| e.into_inner()) {
                        break;
                    }
                    let result = match &mut decider {
                        Ok(decide) => catch_unwind(AssertUnwindSafe(|| decide(&request)))
                            .unwrap_or_else(|_| Err("player decision panicked".into())),
                        Err(error) => Err(error.clone()),
                    };
                    let failed = result.is_err();
                    let gate = delivery.lock().unwrap_or_else(|e| e.into_inner());
                    if failed {
                        // A stale failure can immediately trigger a replacement.
                        // Reject it before publishing, even if this thread is
                        // preempted in the sink. Drop evaluator state afterward.
                        drop(receiver);
                        if *gate {
                            sink(request.reply(result));
                        }
                        return;
                    }
                    if !*gate || !sink(request.reply(result)) {
                        break;
                    }
                }
                // Disconnect submissions before the completion sink is dropped.
                drop(receiver);
            })?;
        Ok(Self {
            sender: Some(sender),
            open,
        })
    }

    pub(crate) fn baseline(
        sink: impl Fn(AiReply) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn(
            || {
                let mut player = player::AiPlayer::new(Box::new(baseline::BaselineEvaluator::new(
                    baseline::Device::flex(),
                )));
                let mut rng = rand::rng();
                Ok(move |request: &AiRequest| {
                    player
                        .decide(&request.board, &request.options, &mut rng)
                        .map_err(|error| error.to_string())
                })
            },
            sink,
        )
    }

    /// A failed handoff returns a completion for the exact reserved flight.
    pub(crate) fn submit(&self, request: AiRequest) -> Result<(), Box<AiReply>> {
        let Some(sender) = &self.sender else {
            return Err(Box::new(request.reply(Err("worker is closed".into()))));
        };
        sender.try_send(request).map_err(|error| {
            Box::new(match error {
                mpsc::TrySendError::Full(request) => {
                    request.reply(Err("worker queue is full".into()))
                }
                mpsc::TrySendError::Disconnected(request) => {
                    request.reply(Err("worker disconnected".into()))
                }
            })
        })
    }

    pub(crate) fn close(&mut self) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.sender.take();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiPlayerDecision, DecisionAction};
    use crate::session::Session;
    use gomoku_core::{Settings, point};
    use std::rc::Rc;
    use std::sync::mpsc;

    fn session() -> Session {
        let mut session = Session::new(&Settings::default());
        session.release_seat(gomoku_core::Color::White);
        session.place(point(7, 7).unwrap());
        session
    }

    fn play() -> DecisionResult {
        Ok(AiPlayerDecision {
            action: DecisionAction::Play(point(7, 8).unwrap()),
            search: None,
            guard: None,
            resignation: None,
        })
    }

    fn receive<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("worker handshake")
    }

    #[test]
    fn stale_flight_coalesces_to_latest_options_without_duplicate_work() {
        let (started, jobs) = mpsc::channel();
        let (release, latch) = mpsc::channel();
        let (done, replies) = mpsc::channel();
        let worker = Worker::spawn(
            move || {
                Ok(move |request: &AiRequest| {
                    started
                        .send((request.options.simulations, request.board.clone()))
                        .unwrap();
                    latch.recv().unwrap();
                    play()
                })
            },
            move |reply| done.send(reply).is_ok(),
        )
        .unwrap();
        let mut session = session();
        worker.submit(session.next_ai_request().unwrap()).unwrap();
        let (_, original) = receive(&jobs);
        for simulations in [2, 9, 23] {
            session
                .set_ai_options(
                    engine::Color::White,
                    crate::ai::AiOptions {
                        simulations,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(session.next_ai_request().is_none());
        }
        assert!(matches!(jobs.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release.send(()).unwrap();
        assert!(!session.apply_ai_reply(receive(&replies)));
        worker.submit(session.next_ai_request().unwrap()).unwrap();
        let (simulations, board) = receive(&jobs);
        assert_eq!(simulations, 23);
        assert_eq!(board, original);
        assert!(session.next_ai_request().is_none());
        release.send(()).unwrap();
        assert!(session.apply_ai_reply(receive(&replies)));
        assert_eq!(session.game().len(), 2);
    }

    #[test]
    fn take_during_search_is_immediate_and_delayed_reply_cannot_act() {
        let (started, jobs) = mpsc::channel();
        let (release, latch) = mpsc::channel();
        let (done, replies) = mpsc::channel();
        let worker = Worker::spawn(
            move || {
                Ok(move |_: &AiRequest| {
                    started.send(()).unwrap();
                    latch.recv().unwrap();
                    play()
                })
            },
            move |reply| done.send(reply).is_ok(),
        )
        .unwrap();
        let mut session = session();
        worker.submit(session.next_ai_request().unwrap()).unwrap();
        receive(&jobs);
        session.take_seat(engine::Color::White);
        session.place(point(8, 8).unwrap());
        assert_eq!(session.game().len(), 2);
        release.send(()).unwrap();
        assert!(!session.apply_ai_reply(receive(&replies)));
        assert_eq!(session.game().stone_at(point(7, 8).unwrap()), None);
        assert!(session.next_ai_request().is_none());
    }

    #[test]
    fn shutdown_never_waits_for_initialization_or_active_decision_and_suppresses_delivery() {
        for active in [false, true] {
            let (started, jobs) = mpsc::channel();
            let (release, latch) = mpsc::channel();
            let (done, replies) = mpsc::channel();
            let mut worker = Worker::spawn(
                move || {
                    if !active {
                        started.send(()).unwrap();
                        latch.recv().unwrap();
                    }
                    Ok(move |_: &AiRequest| {
                        if active {
                            started.send(()).unwrap();
                            latch.recv().unwrap();
                        }
                        play()
                    })
                },
                move |reply| done.send(reply).is_ok(),
            )
            .unwrap();
            let mut session = session();
            if active {
                worker.submit(session.next_ai_request().unwrap()).unwrap();
            }
            receive(&jobs);
            worker.close(); // release is deliberately still held: a join would deadlock.
            release.send(()).unwrap();
            assert!(matches!(
                replies.recv_timeout(std::time::Duration::from_secs(30)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ));
            let reply = worker
                .submit(super::tests::session().next_ai_request().unwrap())
                .unwrap_err();
            assert!(reply.result.is_err());
        }
    }

    #[test]
    fn factory_and_decision_errors_and_panics_retire_and_hold_the_seat() {
        for mode in 0..4 {
            let (done, replies) = mpsc::channel();
            let worker = Worker::spawn(
                move || {
                    assert_ne!(mode, 0, "init panic");
                    if mode == 1 {
                        return Err("init failed".into());
                    }
                    Ok(move |_: &AiRequest| {
                        assert_ne!(mode, 2, "decision panic");
                        Err("decision failed".into())
                    })
                },
                move |reply| done.send(reply).is_ok(),
            )
            .unwrap();
            let mut session = session();
            worker.submit(session.next_ai_request().unwrap()).unwrap();
            assert!(!session.apply_ai_reply(receive(&replies)));
            assert_eq!(
                session.owner(engine::Color::White),
                crate::ai::SeatOwner::Human
            );
            assert!(session.notice.as_ref().unwrap().starts_with("AI error:"));
            assert!(session.next_ai_request().is_none());
        }
    }

    #[test]
    fn full_and_disconnected_submission_returns_exact_reservation_as_error() {
        let (started, jobs) = mpsc::channel();
        let (release, latch) = mpsc::channel();
        let worker = Worker::spawn(
            move || {
                started.send(()).unwrap();
                latch.recv().unwrap();
                Ok(|_: &AiRequest| play())
            },
            |_| false,
        )
        .unwrap();
        receive(&jobs); // factory held, so exactly one queued request fits.
        worker.submit(session().next_ai_request().unwrap()).unwrap();
        let mut other = session();
        let reply = worker.submit(other.next_ai_request().unwrap()).unwrap_err();
        assert!(!other.apply_ai_reply(*reply));
        assert_eq!(
            other.owner(engine::Color::White),
            crate::ai::SeatOwner::Human
        );
        assert!(other.next_ai_request().is_none());
        release.send(()).unwrap();
        drop(worker);

        // Completion sink disappearance ends the worker. A Drop handshake proves
        // its receiver has gone before the disconnected submission.
        struct Exit(mpsc::Sender<()>);
        impl Drop for Exit {
            fn drop(&mut self) {
                self.0.send(()).unwrap();
            }
        }
        let (exit, exited) = mpsc::channel();
        let exit = Exit(exit);
        let worker = Worker::spawn(
            || Ok(|_: &AiRequest| play()),
            move |_| {
                let _ = &exit;
                false
            },
        )
        .unwrap();
        worker.submit(session().next_ai_request().unwrap()).unwrap();
        receive(&exited);
        let mut session = session();
        let request = session.next_ai_request().unwrap();
        let reply = worker.submit(request).unwrap_err();
        assert!(reply.result.as_ref().unwrap_err().contains("disconnected"));
        assert!(!session.apply_ai_reply(*reply));
        assert_eq!(
            session.owner(engine::Color::White),
            crate::ai::SeatOwner::Human
        );
    }

    #[test]
    #[ignore = "manual latency probe: run with --release --ignored --nocapture"]
    fn real_flex_default_4000_visit_latency() {
        let (done, replies) = mpsc::channel();
        let start = std::time::Instant::now();
        let worker = Worker::baseline(move |reply| done.send(reply).is_ok()).unwrap();
        let mut session = session();
        let request = session.next_ai_request().unwrap();
        assert_eq!(request.options.simulations, 4000);
        assert!(request.options.own.enabled);
        assert!(request.options.guard.enabled);
        assert!(request.options.resign);
        worker.submit(request).unwrap();
        let reply = receive(&replies);
        let decision = reply.result.as_ref().unwrap();
        let search = decision.search.as_ref().expect("quiet board searches");
        assert_eq!(search.visits.iter().map(|(_, n)| n).sum::<u32>(), 4000);
        assert!(session.apply_ai_reply(reply));
        eprintln!(
            "real Burn Flex GUI defaults: 400 visits incl init {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn idle_shutdown_closes_request_receiver_without_a_job() {
        let (ready, initialized) = mpsc::channel();
        let (done, replies) = mpsc::channel();
        let mut worker = Worker::spawn(
            move || {
                ready.send(()).unwrap();
                Ok(|_: &AiRequest| -> DecisionResult { panic!("idle worker has no job") })
            },
            move |reply| done.send(reply).is_ok(),
        )
        .unwrap();
        receive(&initialized);
        worker.close();
        assert!(matches!(
            replies.recv_timeout(std::time::Duration::from_secs(30)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn real_flex_worker_searches_requested_visits_then_resigns_losing_fixture() {
        let (done, replies) = mpsc::channel();
        let start = std::time::Instant::now();
        let worker = Worker::baseline(move |reply| done.send(reply).is_ok()).unwrap();
        let mut quiet = session();
        quiet
            .set_ai_options(
                engine::Color::White,
                crate::ai::AiOptions {
                    simulations: 7,
                    own_tss: false, // Force an actual searched tree, not the proof bypass.
                    ..Default::default()
                },
            )
            .unwrap();
        let request = quiet.next_ai_request().unwrap();
        let original = request.board.clone();
        worker.submit(request).unwrap();
        let reply = receive(&replies);
        let decision = reply.result.as_ref().expect("real Flex decision");
        let DecisionAction::Play(mv) = decision.action else {
            panic!("quiet board must play")
        };
        assert!(original.is_legal(mv));
        let search = decision.search.as_ref().expect("genuine MCTS tree");
        assert_eq!(search.visits.iter().map(|(_, n)| n).sum::<u32>(), 7);
        assert_eq!(search.side_to_move, engine::Color::White);
        assert!(decision.guard.is_some());
        assert_eq!(quiet.game().board_snapshot(), original);
        assert!(quiet.apply_ai_reply(reply));
        eprintln!(
            "real Burn Flex quiet: 7 visits incl init {:?}",
            start.elapsed()
        );

        // The owner's main player test fixture: White's open four cannot be
        // stopped by one Black stone. This is heuristic concession, not proof
        // that every legal move loses.
        let mut losing = Session::new(&Settings::default());
        losing
            .load_puzzle(&gomoku_core::Puzzle {
                name: "opponent open four".into(),
                black: [0, 2, 4, 6].map(|col| point(2, col).unwrap()).into(),
                white: [3, 4, 5, 6].map(|col| point(7, col).unwrap()).into(),
                to_move: engine::Color::Black,
                solution: None,
                depth: None,
            })
            .unwrap();
        losing
            .set_ai_options(
                engine::Color::Black,
                crate::ai::AiOptions {
                    simulations: 7,
                    ..Default::default()
                },
            )
            .unwrap();
        losing.release_seat(engine::Color::Black);
        let request = losing.next_ai_request().unwrap();
        assert!(request.options.resign, "GUI default must permit concession");
        assert!(request.options.own.enabled);
        assert!(request.options.guard.enabled);
        let start = std::time::Instant::now();
        worker.submit(request).unwrap();
        let reply = receive(&replies);
        let decision = reply.result.as_ref().unwrap();
        assert!(matches!(decision.action, DecisionAction::Resign(_)));
        assert_eq!(
            decision
                .search
                .as_ref()
                .unwrap()
                .visits
                .iter()
                .map(|(_, n)| n)
                .sum::<u32>(),
            7
        );
        assert!(decision.guard.is_none());
        assert!(losing.apply_ai_reply(reply));
        assert_eq!(losing.game().len(), 0, "concession places no stone");
        assert!(matches!(
            losing.game().outcome(),
            gomoku_core::Outcome::Won {
                winner: engine::Color::White,
                method: gomoku_core::WinMethod::Resignation
            }
        ));
        eprintln!(
            "real Burn Flex losing fixture: 7 visits {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn constructs_non_send_player_in_thread_and_delivers_one_reply() {
        let origin = std::thread::current().id();
        let (tx, rx) = mpsc::channel();
        let worker = Worker::spawn(
            move || {
                assert_ne!(origin, std::thread::current().id());
                let local = Rc::new(point(7, 8).unwrap());
                Ok(move |_: &crate::ai::AiRequest| {
                    Ok(AiPlayerDecision {
                        action: DecisionAction::Play(*local),
                        search: None,
                        guard: None,
                        resignation: None,
                    })
                })
            },
            move |reply| tx.send(reply).is_ok(),
        )
        .unwrap();
        let mut session = session();
        worker.submit(session.next_ai_request().unwrap()).unwrap();
        assert!(session.next_ai_request().is_none());
        assert!(session.apply_ai_reply(rx.recv().unwrap()));
        assert_eq!(session.game().len(), 2);
    }
}
