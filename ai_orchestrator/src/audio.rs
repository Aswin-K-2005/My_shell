use kokoro_micro::TtsEngine;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as async_mpsc;
use tokio::sync::OnceCell;

static AUDIO_TX: OnceCell<async_mpsc::Sender<(String, u64)>> = OnceCell::const_new();
static SINK_HANDLE: OnceCell<Arc<Mutex<Option<Sink>>>> = OnceCell::const_new();

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub async fn init_audio_worker() {
    let (text_tx, mut text_rx) = async_mpsc::channel::<(String, u64)>(100);
    AUDIO_TX.set(text_tx).unwrap();

    let (audio_tx, audio_rx) = std::sync::mpsc::channel::<(Vec<f32>, u64)>();
    let (telemetry_tx, telemetry_rx) = std::sync::mpsc::channel::<(Vec<f32>, u64)>();

    let sink_handle: Arc<Mutex<Option<Sink>>> = Arc::new(Mutex::new(None));
    let _ = SINK_HANDLE.set(Arc::clone(&sink_handle));

    // 1. DEDICATED SPEAKER THREAD
    std::thread::spawn(move || {
        if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&stream_handle) {
                *sink_handle.lock().unwrap() = Some(sink);

                while let Ok((audio_data, gen)) = audio_rx.recv() {
                    if gen != GENERATION.load(Ordering::SeqCst) {
                        continue;
                    }

                    // Hand a clone of the audio to the telemetry pacing thread
                    let _ = telemetry_tx.send((audio_data.clone(), gen));

                    // Queue the audio to the actual hardware speakers
                    if let Some(sink) = sink_handle.lock().unwrap().as_ref() {
                        sink.append(SamplesBuffer::new(1, 24000, audio_data));
                    }
                }
            }
        }
    });

    // 2. DEDICATED TELEMETRY PACING THREAD
    // Chops audio into 20ms slices and paces the UDP packets to perfectly sync with the hardware playback
    std::thread::spawn(move || {
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").ok();
        let window_size = 480; // 20ms at 24,000 Hz sample rate

        while let Ok((audio_data, gen)) = telemetry_rx.recv() {
            if gen != GENERATION.load(Ordering::SeqCst) {
                continue;
            }

            let mut i = 0;
            while i < audio_data.len() {
                // Barge-in check: instantly abort if user interrupts Aish
                if gen != GENERATION.load(Ordering::SeqCst) {
                    break;
                }

                let end = (i + window_size).min(audio_data.len());
                let window = &audio_data[i..end];

                // Calculate RMS (volume) for just this tiny 20ms slice
                let mut sum_squares = 0.0;
                for &sample in window {
                    sum_squares += sample * sample;
                }
                let rms = if window.is_empty() {
                    0.0
                } else {
                    (sum_squares / window.len() as f32).sqrt()
                };

                if let Some(socket) = &udp {
                    let _ = socket.send_to(&rms.to_ne_bytes(), "127.0.0.1:9999");
                }

                // Sleep for exactly 20ms so the telemetry perfectly syncs with the syllable being spoken
                std::thread::sleep(std::time::Duration::from_millis(20));
                i += window_size;
            }

            // Force the visualizer ring to snap shut instantly during long pauses
            if let Some(socket) = &udp {
                let zero: f32 = 0.0;
                let _ = socket.send_to(&zero.to_ne_bytes(), "127.0.0.1:9999");
            }
        }
    });

    // 3. ASYNC TTS GENERATOR TASK
    tokio::spawn(async move {
        println!("  [JARVIS] Pre-loading Neural Voice Engine...");
        let mut tts = TtsEngine::new().await.expect("Failed to initialize TTS");
        let boot_gen = GENERATION.load(Ordering::SeqCst);
        if let Ok(audio_data) = tts.synthesize_with_options(
            "All systems online.",
            Some("af_sarah"),
            1.5,
            1.0,
            Some("en"),
        ) {
            let _ = audio_tx.send((audio_data, boot_gen));
        }

        while let Some((text, gen)) = text_rx.recv().await {
            if gen != GENERATION.load(Ordering::SeqCst) {
                continue;
            }
            let (speed, pitch) = speech_params_for(&text);
            if let Ok(audio_data) =
                tts.synthesize_with_options(&text, Some("af_sarah"), speed, pitch, Some("en"))
            {
                let _ = audio_tx.send((audio_data, gen));
            }
        }
    });
}

fn speech_params_for(text: &str) -> (f32, f32) {
    let technical_density = text
        .split_whitespace()
        .filter(|w| {
            w.contains('/')
                || w.contains('_')
                || w.contains('.')
                || w.chars().any(|c| c.is_ascii_digit())
        })
        .count();
    let word_count = text.split_whitespace().count().max(1);
    let ratio = technical_density as f32 / word_count as f32;
    if ratio > 0.25 {
        (1.15, 1.0)
    } else {
        (1.35, 1.0)
    }
}

pub fn speak_stream(text: String) {
    if let Some(tx) = AUDIO_TX.get() {
        let gen = GENERATION.load(Ordering::SeqCst);
        let _ = tx.try_send((text, gen));
    }
}

pub fn stop_speaking() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(handle) = SINK_HANDLE.get() {
        if let Ok(guard) = handle.lock() {
            if let Some(sink) = guard.as_ref() {
                sink.stop();
            }
        }
    }
}
