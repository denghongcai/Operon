use super::{OnboardArgs, OnboardPlan, Prompt};

pub(super) fn build_onboard_plan(
    args: OnboardArgs,
    prompt: &mut impl Prompt,
) -> anyhow::Result<OnboardPlan> {
    super::build_onboard_plan_inner(args, prompt)
}
