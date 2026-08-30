use stt::{ListenMode, ListenerStatus};
use tts::Speech;
use voice::bootstrap;

fn main() {
    let (mut assistant, speech, _chatterbox_supervisor) = bootstrap::build();

    println!("Cargando el pipeline de escucha (VAD + wake word)...");
    let listener = bootstrap::build_listener();
    let mut listener = listener.with_status(|status| match status {
        ListenerStatus::Listening => println!("👂 Escuchando... (decí 'oye Nala')"),
        ListenerStatus::Heard => println!("🎙️  ¡Te escuché!"),
        ListenerStatus::Capturing => {}
        ListenerStatus::Transcribing => println!("🤔 Procesando..."),
    });

    let greeting = "Hola, en que te puedo ayudar?";
    println!("{greeting}");
    let _ = speech.say(greeting);

    // No cancel-signal handler here on purpose: it would swallow every
    // Ctrl+C to cancel the current turn instead, and with no keyboard
    // left in this loop (unlike push-to-talk's "salir") that leaves no
    // way to exit the process at all. Ctrl+C keeps its default OS
    // behaviour instead — it just closes the app, same as any other CLI.
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
