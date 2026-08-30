use nala::cli::prompt::MultilineReader;
use tts::Speech;
use voice::bootstrap;

fn main() {
    let (assistant, speech, _chatterbox_supervisor) = bootstrap::build();
    let (mut assistant, cancel_signal) = nala::bootstrap::install_cancel_signal(assistant);

    let mut reader = MultilineReader::new();

    let greeting = "Hola, en que te puedo ayudar?";
    println!("{greeting}");
    let _ = speech.say(greeting);

    loop {
        println!(
            "(puedes escribir/pegar varias lineas y usar flechas/backspace entre ellas; Ctrl+Enter envia)"
        );

        let input = match reader.read().expect("Failed reading input") {
            Some(input) => input,
            None => break,
        };

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
