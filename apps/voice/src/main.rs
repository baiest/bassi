use nala::application::assistant::AssistantError;
use nala::ports::llm::LlmError;
use tts::Speech;
use voice::bootstrap;

fn main() {
    let (assistant, speech, _chatterbox_supervisor) = bootstrap::build();
    let (mut assistant, cancel_signal) = nala::bootstrap::install_cancel_signal(assistant);

    let transcriber = bootstrap::build_transcriber();

    let greeting = "Hola, en que te puedo ayudar?";
    println!("{greeting}");
    let _ = speech.say(greeting);
    speech.flush();

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
                // this further would only add extra round-trips. flush()
                // blocks until it's fully spoken, keeping the next
                // recording from picking up Nala's own voice.
                let _ = speech.say(&response);
                speech.flush();
            }
            Err(e) => {
                eprintln!("Error: {e}");
                let spoken = match &e {
                    AssistantError::Llm(LlmError::ModelNotFound(model)) => format!(
                        "No encontré el modelo {model} en Ollama. Corré 'ollama pull {model}' y probá de nuevo."
                    ),
                    AssistantError::Llm(LlmError::RequestFailed(_)) => {
                        "No pude conectarme con Ollama. Revisá que esté corriendo.".to_string()
                    }
                    AssistantError::Llm(LlmError::InvalidResponse(_)) => {
                        "Ollama me respondió algo que no pude entender.".to_string()
                    }
                    AssistantError::Cancelled => "Cancelado.".to_string(),
                    _ => "Tuve un error interno, revisá la consola.".to_string(),
                };
                let _ = speech.say(&spoken);
                speech.flush();
            }
        }
    }

    speech.flush();
}
