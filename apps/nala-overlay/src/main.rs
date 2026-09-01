#[cfg(windows)]
mod overlay;

fn main() {
    #[cfg(windows)]
    {
        if let Err(error) = overlay::run() {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!(
            "nala-overlay only runs on Windows (it draws over a real Windows desktop \
             and needs mic/speaker access)."
        );
        std::process::exit(1);
    }
}
