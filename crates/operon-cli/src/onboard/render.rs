use super::{OnboardPlan, OnboardRole, OnboardSummary};

pub(super) fn onboard_plan_summary(plan: &OnboardPlan) -> OnboardSummary {
    OnboardSummary {
        role: format!("{:?}", plan.role).to_ascii_lowercase(),
        output_dir: plan.output_dir.display().to_string(),
        files: plan
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect(),
        daemon_command: plan.daemon_command.clone(),
        equivalent_cli: plan.equivalent_cli.clone(),
        completion_commands: plan.completion_commands.clone(),
        advertise_lan_default: matches!(plan.role, OnboardRole::Daemon | OnboardRole::Both)
            .then_some(true),
        advertise_lan_note: matches!(plan.role, OnboardRole::Daemon | OnboardRole::Both)
            .then(|| onboard_advertise_lan_note().to_string()),
    }
}

pub(super) fn print_onboard_plan(plan: &OnboardPlan) {
    println!("Wrote:");
    for file in &plan.files {
        println!("  {}", file.path.display());
    }

    if let Some(command) = &plan.daemon_command {
        println!();
        println!("Next:");
        println!("  {command}");
        println!("  {}", onboard_advertise_lan_note());
    }

    println!();
    println!("Equivalent CLI:");
    for command in &plan.equivalent_cli {
        println!("  {command}");
    }

    println!();
    println!("Shell completion:");
    for command in &plan.completion_commands {
        println!("  {command}");
    }
}

pub(super) fn completion_setup_commands() -> Vec<String> {
    vec![
        "mkdir -p ~/.local/share/bash-completion/completions && operon completion bash > ~/.local/share/bash-completion/completions/operon".to_string(),
        "mkdir -p ~/.zfunc && operon completion zsh > ~/.zfunc/_operon".to_string(),
    ]
}

pub(super) fn onboard_advertise_lan_note() -> &'static str {
    "advertise_lan=true: onboarding advertises daemon endpoints on LAN for first-run discovery"
}
