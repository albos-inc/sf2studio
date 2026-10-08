//! MIDI keyboard input: messages go straight to the live synthesizer (no
//! UI delay) and are also handed to the UI.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use midir::{MidiInput, MidiInputConnection};

use crate::audio::Transport;

pub struct MidiIn {
    connection: Option<(String, MidiInputConnection<()>)>,
    sender: Sender<Vec<u8>>,
    pub receiver: Receiver<Vec<u8>>,
}

impl MidiIn {
    pub fn new() -> MidiIn {
        let (sender, receiver) = channel();
        MidiIn { connection: None, sender, receiver }
    }

    /// Names of the MIDI inputs.
    pub fn ports() -> Vec<String> {
        let Ok(input) = MidiInput::new("sf2studio") else { return Vec::new() };
        input.ports().iter().filter_map(|p| input.port_name(p).ok()).collect()
    }

    pub fn connected(&self) -> Option<&str> {
        self.connection.as_ref().map(|(name, _)| name.as_str())
    }

    pub fn disconnect(&mut self) {
        if let Some((_, connection)) = self.connection.take() {
            connection.close();
        }
    }

    pub fn connect(
        &mut self,
        name: &str,
        transport: Arc<Mutex<Transport>>,
        repaint: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        self.disconnect();
        let input = MidiInput::new("sf2studio").map_err(|e| e.to_string())?;
        let port = input
            .ports()
            .into_iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(name))
            .ok_or_else(|| format!("{name} is gone"))?;
        let sender = self.sender.clone();
        let connection = input
            .connect(
                &port,
                "sf2studio-in",
                move |_, message, _| {
                    // Channel messages only; every channel plays channel 0.
                    let Some(&status) = message.first() else { return };
                    if !(0x80..0xf0).contains(&status) {
                        return;
                    }
                    let mut message = message.to_vec();
                    message[0] = status & 0xf0;
                    if let Ok(mut transport) = transport.lock()
                        && let Some(live) = &mut transport.live
                    {
                        live.synth.process_midi_message(&message);
                    }
                    let _ = sender.send(message);
                    repaint();
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        self.connection = Some((name.to_string(), connection));
        Ok(())
    }
}
