use vestrace_fault_scenario::{ScenarioSettings, child};

#[tokio::main]
async fn main() {
    let settings = match ScenarioSettings::from_env_and_args(std::env::args().skip(1), &|name| {
        std::env::var(name).ok()
    }) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    if settings.is_child() {
        let dispatch_url = match child::dispatch_url_argument(std::env::args().skip(1)) {
            Ok(url) => url,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
        // Diverges: the child always ends in `abort()`.
        child::run_child(&settings, &dispatch_url).await;
    }

    // Task 4 gives the parent its own.
    eprintln!(
        "scenario parent not yet implemented for {:?}",
        settings.point()
    );
    std::process::exit(3);
}
