use stt::ListenMode;
use tts::Speech;
use voice::bootstrap;

fn main() {
    let (assistant, speech, _chatterbox_supervisor) = bootstrap::build();
    let (mut assistant, cancel_signal) = nala::bootstrap::install_cancel_signal(assistant);

    println!("Cargando el pipeline de escucha (VAD + wake word)...");
    let mut listener = bootstrap::build_listener();

    let greeting = "Hola, en que te puedo ayudar?";
    println!("{greeting}");
    let _ = speech.say(greeting);

    // Ctrl+C cancels the current turn, not the process — closing the
    // terminal is the exit path for an always-listening front end, same
    // as any other voice assistant with no keyboard in the loop.
    let mut mode = ListenMode::WakeWord;
    loop {
        let heard = match listener.listen(mode) {
            Ok(heard) => heard,
            Err(e) => {
                eprintln!("Error escuchando: {e}");
                break;
            }
        };

        let Some(input) = heard else {
            // A follow-up window expired, or its capture didn't pass the
            // sanity filter — either way, go back to requiring the wake
            // phrase rather than leaving the mic open indefinitely.
            mode = ListenMode::WakeWord;
            continue;
        };

        println!("Vos: {input}");

        #[cfg(windows)]
        if let Some(signal) = &cancel_signal {
            signal.reset();
        }
        #[cfg(not(windows))]
        let _ = &cancel_signal;

        match assistant.process(&input) {
            Ok(response) => {
                println!("{response}");
                // The whole answer goes out in a single `say` call: a
                // streaming backend starts playback on the first chunk
                // rather than waiting for the full answer, so splitting
                // this further would only add extra round-trips. flush()
                // blocks until it's fully spoken, which is what keeps the
                // microphone from hearing Nala's own voice once listening
                // resumes.
                let _ = speech.say(&response);
                speech.flush();
                mode = ListenMode::FollowUp;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                mode = ListenMode::WakeWord;
            }
        }
    }

    speech.flush();
}
