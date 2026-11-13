//! Many questions to a bus from one thread, with at most so many waiting at a time
//! (`OBS-258`).
//!
//! The look of the focus asks the bus of accessibility two questions of every element it
//! visits. Asked one after the other, a look costs the number of questions times the journey
//! of one, so an application that answers each question at once is still slow to look at:
//! Firefox on a page it had shown for a while, 154 questions, 256 ms in the median. The same
//! questions asked a level of the tree at a time took 70 ms (2026-11-13,
//! `tools/sondy/linux/atspi_rownolegle.py`).
//!
//! The library of the bus files every reply under the number of its question and reads the
//! socket on a thread of its own, so the questions here are only started and waited for.
//! Nothing in this file knows the bus: it runs futures, and its tests run on every system.

// The Linux route is the one that asks, and it is not built elsewhere.
#![cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "the Linux route asks what the tests of other systems do not"
    )
)]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::sync::Arc;
use std::task::Wake;
use std::thread::Thread;

/// Runs `future` on this thread to its end, sleeping while it waits. What it waits for
/// wakes this thread from another one.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = core::pin::pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        // A wake that came before this is kept by the thread, so this returns at once.
        std::thread::park();
    }
}

/// Wakes the thread that waits in [`block_on`].
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Waits for `questions`, started in their order with at most `at_once` of them waiting at a
/// time: one that is answered makes room for the next. A question is started only while
/// `may_start` says so, and every question started is waited for to its end. The answers
/// come in the order of the questions, `None` for one that was never started.
pub(crate) async fn at_most<F: Future>(
    at_once: usize,
    questions: Vec<F>,
    mut may_start: impl FnMut() -> bool,
) -> Vec<Option<F::Output>> {
    let at_once = at_once.max(1);
    let mut waiting: Vec<Option<Pin<Box<F>>>> = questions
        .into_iter()
        .map(|question| Some(Box::pin(question)))
        .collect();
    let mut answers: Vec<Option<F::Output>> = waiting.iter().map(|_| None).collect();
    // The questions before `started` have been started, and the ones before `done` are
    // answered: only the ones between are asked again when something wakes.
    let mut started = 0;
    let mut done = 0;
    let mut running = 0;
    core::future::poll_fn(|context| {
        loop {
            while running < at_once && started < waiting.len() && may_start() {
                started += 1;
                running += 1;
            }
            let mut answered = false;
            let range = done..started;
            for (slot, answer) in waiting[range.clone()].iter_mut().zip(&mut answers[range]) {
                if let Some(Poll::Ready(said)) = slot
                    .as_mut()
                    .map(|question| question.as_mut().poll(context))
                {
                    *answer = Some(said);
                    *slot = None;
                    running -= 1;
                    answered = true;
                }
            }
            while done < started && waiting[done].is_none() {
                done += 1;
            }
            if !answered {
                // Nothing new: the ones still waiting wake this, or nothing is left to start.
                return if running == 0 {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                };
            }
        }
    })
    .await;
    answers
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A question that is answered `value` after it was asked `left` more times, and wakes
    /// itself so that it is asked again.
    struct Countdown {
        left: usize,
        value: char,
    }

    impl Future for Countdown {
        type Output = char;

        fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<char> {
            if self.left == 0 {
                return Poll::Ready(self.value);
            }
            self.left -= 1;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }

    /// How many questions wait at a time, and the most that ever did.
    #[derive(Default)]
    struct Count {
        now: AtomicUsize,
        most: AtomicUsize,
    }

    /// A question that counts itself as waiting from its first asking to its answer.
    struct Counted<'c> {
        inner: Countdown,
        count: &'c Count,
        asked: bool,
    }

    impl Future for Counted<'_> {
        type Output = char;

        fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<char> {
            if !self.asked {
                self.asked = true;
                let now = self.count.now.fetch_add(1, Ordering::SeqCst) + 1;
                self.count.most.fetch_max(now, Ordering::SeqCst);
            }
            let said = Pin::new(&mut self.inner).poll(context);
            if said.is_ready() {
                self.count.now.fetch_sub(1, Ordering::SeqCst);
            }
            said
        }
    }

    fn countdowns(lefts: &[usize]) -> Vec<Countdown> {
        lefts
            .iter()
            .zip('a'..)
            .map(|(left, value)| Countdown { left: *left, value })
            .collect()
    }

    #[test]
    fn the_answers_come_in_the_order_of_the_questions_whatever_order_they_came_in() {
        let answers = block_on(at_most(2, countdowns(&[5, 0, 2, 1, 0]), || true));
        assert_eq!(answers, ['a', 'b', 'c', 'd', 'e'].map(Some));
    }

    #[test]
    fn no_more_than_the_limit_wait_at_a_time_and_the_limit_is_used() {
        for at_once in [1, 3, 32] {
            let count = Count::default();
            let questions: Vec<Counted<'_>> = countdowns(&[3, 1, 4, 1, 5, 9, 2, 6])
                .into_iter()
                .map(|inner| Counted {
                    inner,
                    count: &count,
                    asked: false,
                })
                .collect();
            let answers = block_on(at_most(at_once, questions, || true));
            assert!(answers.iter().all(Option::is_some), "{at_once}");
            let most = count.most.load(Ordering::SeqCst);
            assert_eq!(most, at_once.min(8), "{at_once} at once, {most} waited");
        }
    }

    #[test]
    fn a_limit_of_nothing_still_asks_one_at_a_time() {
        let answers = block_on(at_most(0, countdowns(&[1, 0]), || true));
        assert_eq!(answers, [Some('a'), Some('b')]);
    }

    #[test]
    fn a_question_not_started_has_no_answer_and_the_ones_started_are_waited_for() {
        // Room to start three, and the fourth is refused: the first three are waited for to
        // their end, the rest are never started.
        let mut allowed = 3_usize;
        let answers = block_on(at_most(4, countdowns(&[6, 2, 4, 0, 0]), || {
            let may = allowed > 0;
            allowed = allowed.saturating_sub(1);
            may
        }));
        assert_eq!(answers, [Some('a'), Some('b'), Some('c'), None, None]);
    }

    #[test]
    fn no_questions_are_no_answers() {
        let answers = block_on(at_most(4, countdowns(&[]), || true));
        assert!(answers.is_empty());
    }

    /// A question answered by another thread, which wakes the one that waits.
    struct FromAnotherThread {
        shared: Arc<Mutex<(bool, Option<Waker>)>>,
    }

    impl Future for FromAnotherThread {
        type Output = ();

        fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
            let mut shared = self.shared.lock().expect("not poisoned");
            if shared.0 {
                return Poll::Ready(());
            }
            shared.1 = Some(context.waker().clone());
            Poll::Pending
        }
    }

    #[test]
    fn a_thread_that_sleeps_is_woken_by_the_answer_from_another_one() {
        let shared = Arc::new(Mutex::new((false, None::<Waker>)));
        let answering = Arc::clone(&shared);
        let answerer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            let waker = {
                let mut shared = answering.lock().expect("not poisoned");
                shared.0 = true;
                shared.1.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        block_on(FromAnotherThread { shared });
        answerer.join().expect("the answering thread ended");
    }
}
