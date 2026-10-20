use leptos::{html, prelude::*, task::spawn_local};
use protocol::{LightState, Mode, Rgb};

use crate::api;

#[component]
pub fn App() -> impl IntoView {
    let state = RwSignal::new(None::<LightState>);
    let error = RwSignal::new(None::<String>);

    spawn_local(async move {
        match api::state().await {
            Ok(s) => state.set(Some(s)),
            Err(e) => error.set(Some(e)),
        }
    });

    view! {
        <main>
            <h1>"wled-rs"</h1>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            <Show
                when=move || state.with(Option::is_some)
                fallback=|| view! { <p class="muted">"Connecting…"</p> }
            >
                <LightControls state error />
            </Show>
            <Firmware />
        </main>
    }
}

#[component]
fn LightControls(state: RwSignal<Option<LightState>>, error: RwSignal<Option<String>>) -> impl IntoView {
    let saver = Saver::new(error);
    let current = move || state.get().unwrap_or_default();
    let update = move |f: &dyn Fn(&mut LightState)| {
        state.update(|s| {
            if let Some(s) = s {
                f(s);
                saver.save(*s);
            }
        });
    };

    // Remembered so switching away from RGB and back restores the colour.
    let rgb = RwSignal::new(match current().mode {
        Mode::Rgb(c) => c,
        _ => Rgb::new(255, 0, 128),
    });

    let modes: [(&str, fn(Rgb) -> Mode); 4] = [
        ("Off", |_| Mode::Off),
        ("Warm", |_| Mode::Warm),
        ("Cool", |_| Mode::Cool),
        ("RGB", Mode::Rgb),
    ];
    let mode_buttons = modes
        .into_iter()
        .map(|(label, make)| {
            let active = move || same_kind(current().mode, make(rgb.get()));
            view! {
                <button
                    class:active=active
                    on:click=move |_| update(&|s| s.mode = make(rgb.get_untracked()))
                >
                    {label}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class="modes">{mode_buttons}</div>

        <label class="field">
            "Brightness"
            <input
                type="range"
                min="1"
                max="255"
                prop:value=move || current().brightness.to_string()
                disabled=move || current().mode == Mode::Off
                on:input=move |ev| {
                    if let Ok(b) = event_target_value(&ev).parse() {
                        update(&|s| s.brightness = b);
                    }
                }
            />
        </label>

        <Show when=move || matches!(current().mode, Mode::Rgb(_))>
            <label class="field">
                "Colour"
                <input
                    type="color"
                    prop:value=move || rgb.get().to_hex()
                    on:input=move |ev| {
                        if let Some(c) = Rgb::from_hex(&event_target_value(&ev)) {
                            rgb.set(c);
                            update(&|s| s.mode = Mode::Rgb(c));
                        }
                    }
                />
            </label>
        </Show>
    }
}

#[derive(Clone)]
enum Ota {
    Idle,
    Uploading(String),
    Rebooting,
    Failed(String),
}

#[component]
fn Firmware() -> impl IntoView {
    let info = LocalResource::new(api::info);
    let ota = RwSignal::new(Ota::Idle);
    let file_input = NodeRef::<html::Input>::new();

    let upload = move |_| {
        let Some(file) = file_input.get().and_then(|i| i.files()).and_then(|f| f.get(0)) else {
            return;
        };
        ota.set(Ota::Uploading(file.name()));
        spawn_local(async move {
            ota.set(match api::upload_firmware(file).await {
                Ok(()) => Ota::Rebooting,
                Err(e) => Ota::Failed(e),
            });
        });
    };

    view! {
        <details>
            <summary>"Firmware"</summary>
            <p class="muted">
                {move || match info.get() {
                    Some(Ok(i)) => format!("Version {} · slot {} · {} LEDs", i.version, i.ota_slot, i.led_count),
                    Some(Err(e)) => e,
                    None => String::new(),
                }}
            </p>
            {move || match ota.get() {
                Ota::Idle | Ota::Failed(_) => view! {
                    {match ota.get() {
                        Ota::Failed(e) => Some(view! { <p class="error">"Update failed: " {e}</p> }),
                        _ => None,
                    }}
                    <input type="file" accept=".bin" node_ref=file_input on:change=upload />
                }.into_any(),
                Ota::Uploading(name) => view! { <p>"Flashing " {name} "…"</p> }.into_any(),
                Ota::Rebooting => view! {
                    <p>"Update installed. The device is rebooting."</p>
                    <button on:click=|_| { let _ = window().location().reload(); }>"Reload"</button>
                }.into_any(),
            }}
        </details>
    }
}

/// Keeps at most one state write in flight; changes made meanwhile collapse into the newest one,
/// so dragging a slider doesn't queue up hundreds of requests on the ESP32.
#[derive(Clone, Copy)]
struct Saver {
    in_flight: StoredValue<bool>,
    pending: StoredValue<Option<LightState>>,
    error: RwSignal<Option<String>>,
}

impl Saver {
    fn new(error: RwSignal<Option<String>>) -> Self {
        Self {
            in_flight: StoredValue::new(false),
            pending: StoredValue::new(None),
            error,
        }
    }

    fn save(self, state: LightState) {
        if self.in_flight.get_value() {
            self.pending.set_value(Some(state));
            return;
        }
        self.in_flight.set_value(true);
        spawn_local(async move {
            let mut next = Some(state);
            while let Some(state) = next {
                self.error.set(api::set_state(&state).await.err());
                next = None;
                self.pending.update_value(|p| next = p.take());
            }
            self.in_flight.set_value(false);
        });
    }
}

fn same_kind(a: Mode, b: Mode) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}
