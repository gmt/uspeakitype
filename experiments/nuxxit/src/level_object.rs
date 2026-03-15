use core::pin::Pin;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(f64, level)]
        #[qproperty(f64, peak)]
        #[qproperty(QString, status)]
        #[qproperty(QString, model_label)]
        #[qproperty(bool, waterfallish)]
        #[qproperty(i32, frame)]
        type LevelBridge = super::LevelBridgeRust;

        #[qinvokable]
        fn tick(self: Pin<&mut Self>);
    }
}

#[derive(Default)]
pub struct LevelBridgeRust {
    level: f64,
    peak: f64,
    status: QString,
    model_label: QString,
    waterfallish: bool,
    frame: i32,
}

impl qobject::LevelBridge {
    pub fn tick(mut self: Pin<&mut Self>) {
        let next_frame = *self.frame() + 1;
        let phase = next_frame as f64 / 9.0;
        let level = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let peak = ((phase * 0.37).cos() * 0.5 + 0.5).clamp(level, 1.0);
        let waterfallish = (next_frame / 45) % 2 == 1;

        self.as_mut().set_frame(next_frame);
        self.as_mut().set_level(level);
        self.as_mut().set_peak(peak);
        self.as_mut().set_waterfallish(waterfallish);
        self.as_mut()
            .set_model_label(QString::from("moonshine-base (fake)"));
        self.as_mut().set_status(QString::from(format!(
            "{} · {:.0}% level · {:.0}% peak",
            if waterfallish { "waterfallish" } else { "meter" },
            level * 100.0,
            peak * 100.0
        )));
    }
}
