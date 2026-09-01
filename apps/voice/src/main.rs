use agent_protocol::EventSink;
use tts::Speech;
use voice::bootstrap;
use voice::client::ClientError;

/// Default bind address for `voice --serve` (a phone client connects
/// here), overridable with `NALA_VOICE_ADDR`.
const DEFAULT_VOICE_ADDR: &str = "127.0.0.1:4181";
/// Default address `voice --serve` forwards turns to, overridable with
/// `NALA_ADDR` — matches `nala --serve`'s own default.
const DEFAULT_NALA_ADDR: &str = "127.0.0.1:4180";

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--serve") {
        let voice_addr =
            std::env::var("NALA_VOICE_ADDR").unwrap_or_else(|_| DEFAULT_VOICE_ADDR.to_string());
        let nala_addr =
            std::env::var("NALA_ADDR").unwrap_or_else(|_| DEFAULT_NALA_ADDR.to_string());
        if let Err(error) = voice::audio_server::serve(&voice_addr, &nala_addr) {
            eprintln!("Error: could not start the server on {voice_addr}: {error}");
            std::process::exit(1);
        }
        return;
    }

    let (mut client, mut events, speech, _chatterbox_supervisor) = match bootstrap::build() {
        Ok(built) => built,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    };

    let transcriber = bootstrap::build_transcriber();

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

        match client.send(input.trim(), |event| events.emit(event)) {
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
                    ClientError::Connect(_) | ClientError::Io(_) => {
                        "No pude conectarme con Nala. Revisá que esté corriendo.".to_string()
                    }
                    ClientError::Server(_) | ClientError::ClosedWithoutReply => {
                        "Nala tuvo un error interno, revisá la consola.".to_string()
                    }
                };
                let _ = speech.say(&spoken);
                speech.flush();
            }
        }
    }

    speech.flush();
}
