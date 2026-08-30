use tts::Speech;
use voice::bootstrap;

fn main() {
    let (assistant, speech, _chatterbox_supervisor) = bootstrap::build();
    let (mut assistant, cancel_signal) = nala::bootstrap::install_cancel_signal(assistant);

    let transcriber = bootstrap::build_transcriber();

    let greeting = "Hola, en que te puedo ayudar?";
    println!("{greeting}");
    let _ = speech.say(greeting);

    loop {
        println!("Apretá Enter para hablar (o escribí 'salir' para terminar)...");
        let mut trigger = String::new();
        match std::io::stdin().read_line(&mut trigger) {
            Ok(0) => break, // stdin closed (EOF)
            Ok(_) if trigger.trim().eq_ignore_ascii_case("salir") => break,
            _ => {}
        }

        let audio = match stt::record_until_enter() {
            Ok(audio) => audio,
            Err(e) => {
                eprintln!("Error grabando: {e}");
                continue;
            }
        };

        let input = match transcriber.transcribe(&audio.samples) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => {
                println!("No se entendió nada, probá de nuevo.");
                continue;
            }
            Err(e) => {
                eprintln!("Error transcribiendo: {e}");
                continue;
            }
        };
        println!("Vos: {input}");

        #[cfg(windows)]
        if let Some(signal) = &cancel_signal {
            signal.reset();
        }
        #[cfg(not(windows))]
        let _ = &cancel_signal;

        match assistant.process(input.trim()) {
            Ok(response) => {
                println!("{response}");
                // The whole answer goes out in a single `say` call: a
                // streaming backend starts playback on the first chunk
                // rather than waiting for the full answer, so splitting
                // this further would only add extra round-trips.
                let _ = speech.say(&response);
            }
            Err(e) => eprintln!("Error: {e}"),
        }
    }

    speech.flush();
}
