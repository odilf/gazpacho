use libtest_mimic::{Arguments, Completion, Conclusion, Trial};
use rand::{rngs::SmallRng, seq::SliceRandom};

use super::TestVideo;

#[derive(Debug)]
pub struct Property<M> {
    pub name: &'static str,
    pub trial: fn(&TestVideo<M>) -> eyre::Result<()>,
    pub cost: u32,
}

// Imperfect derive
impl<M> Clone for Property<M> {
    fn clone(&self) -> Self {
        Property {
            name: self.name,
            trial: self.trial,
            cost: self.cost,
        }
    }
}
impl<M> Copy for Property<M> {}

#[macro_export]
macro_rules! props {
    ([$($prop:expr, $cost:expr);* $(;)?], $fixtures:expr) => {
        (
            &[$(
                $crate::video::test_harness::Property {
                    name: stringify!($prop),
                    trial: $prop,
                    cost: $cost,
                }
            ),*],
            $fixtures
        )
    };

    ([$($prop:expr, $cost:expr);* $(;)?]) => {
        $crate::props!([$($prop, $cost);*], $crate::videos().iter().collect())
    };
}

#[macro_export]
macro_rules! test_video_properties {
    ($($item:expr);* $(;)?) => {
        fn main() {
            $crate::video::test_harness::run(move |budget, mut rng| {
                std::iter::empty()
                    $(.chain({
                        let (properties, fixtures) = $item;
                        $crate::video::test_harness::test_properties(properties, fixtures, budget, &mut rng)
                    }))*
            }).exit()
        }
    };
}

pub fn run<I, F>(f: F) -> Conclusion
where
    F: FnOnce(Option<u64>, &mut SmallRng) -> I,
    I: Iterator<Item = Trial>,
{
    let args = Arguments::from_args();
    crate::init_tracing_stderr();
    let budget = super::budget::get_budget();
    let mut rng = super::budget::rng();

    let tests = f(budget, &mut rng);

    libtest_mimic::run(&args, tests.collect())
}

// NIT: This could return an iterator...
pub fn test_properties<'a, R: rand::Rng, M: 'static + Sync>(
    // (properties, fixtures): (&'static [Property<M>], Vec<&'static TestVideo<M>>),
    properties: &[Property<M>],
    fixtures: Vec<&'static TestVideo<M>>,
    budget: Option<u64>,
    rng: &'a mut R,
) -> Vec<Trial>
where
    M: std::fmt::Debug,
{
    properties
        .into_iter()
        .flat_map(move |&prop| {
            let mut accumulated_cost = 0;

            // TODO(test): This oversamples chromium videos.
            let mut fixtures = fixtures.clone();
            fixtures.shuffle(rng);

            fixtures
                .into_iter()
                // TODO: Don't just skip, eventually.
                .filter(|video| video.failed.is_none())
                .filter(move |video| {
                    let Some(cost) = video.cost else { return false };
                    let cost = u64::from(prop.cost) * cost.get();
                    if budget.is_some_and(|budget| accumulated_cost + cost > budget) {
                        return false;
                    }
                    accumulated_cost += cost;
                    true
                })
                .map(move |video| {
                    Trial::ignorable_test(
                        format!("{}::{}/{}", prop.name, video.category, video.name),
                        move || {
                            // TODO: This doesn't seem to show up as ignored?
                            if let Some(reason) = video.failed.as_ref() {
                                return Ok(Completion::ignored_with(reason));
                            }

                            (prop.trial)(&video).map_err(|err| format!("{err:?}"))?;
                            Ok(Completion::Completed)
                        },
                    )
                })
        })
        .collect()
}
