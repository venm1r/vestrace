use vestrace_fault_scenario::ScenarioSettings;

fn main() {
    let settings = match ScenarioSettings::from_env_and_args(std::env::args().skip(1), &|name| {
        std::env::var(name).ok()
    }) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    // Task 3 gives the child its work and Task 4 gives the parent its own.
    eprintln!(
        "scenario not yet implemented for {:?} (child: {})",
        settings.point(),
        settings.is_child()
    );
    std::process::exit(3);
}
