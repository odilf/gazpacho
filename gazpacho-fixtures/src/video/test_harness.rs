use libtest_mimic::{Arguments, Completion, Trial};

use super::TestVideo;

type Property<M> = (&'static str, fn(&TestVideo<M>) -> eyre::Result<()>);

#[macro_export]
macro_rules! props {
    ([$($prop:ident),* $(,)?], $fixtures:expr) => {
        $crate::video::test_properties(
            &[$(
                (stringify!($prop), $prop)
            ),*],
            $fixtures
        )
    };

    ([$($prop:ident),* $(,)?]) => {
        $crate::props!([$($prop),*], $crate::videos().iter())
    };
}

#[macro_export]
macro_rules! test_video_properties {
    ($($item:expr);* $(;)?) => {
        fn main() {
            $crate::video::run_tests(
                std::iter::empty()
                    $(.chain($item))*
            )
        }
    };
}

pub fn run_tests(trials: impl Iterator<Item = Trial>) {
    let args = Arguments::from_args();
    crate::init_tracing_stderr();
    libtest_mimic::run(&args, trials.collect()).exit();
}

pub fn test_properties<'a, M: 'static + Sync>(
    properties: &[Property<M>],
    fixtures: impl IntoIterator<Item = &'static TestVideo<M>>,
) -> impl Iterator<Item = Trial> {
    fixtures.into_iter().flat_map(|video| {
        properties.into_iter().map(move |&(name, property)| {
            Trial::ignorable_test(format!("{name}::{}", video.name), move || {
                if let Some(reason) = video.failed.as_ref() {
                    return Ok(Completion::ignored_with(reason));
                }

                property(&video).map_err(|err| format!("{err:?}"))?;
                Ok(Completion::Completed)
            })
        })
    })

    // XXX: Consider this
    // // Synthetic clips that failed to generate are surfaced as ignored.
    // for (video, reason) in fixtures.failed_specs() {
    //     trials.push(Trial::ignorable_test(
    //         format!("not_generated::{}", video.name),
    //         move || Ok(Completion::ignored_with(reason)),
    //     ));
    // }
}
