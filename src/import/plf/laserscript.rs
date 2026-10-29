//! LaserScript (`.ls`): JavaScript that draws with `move(x, y)` and
//! `line(x, y)` in millimetres, like VisiCut's `LaserScriptImporter` and
//! LibLaserCut's `LaserScriptBootstrap.js`.
use super::script::{self, Engine};
use crate::{i18n::tr, tr};
use boa_engine::{
    Context, JsArgs, JsNativeError, JsResult, JsString, JsValue, NativeFunction, js_string,
};
use std::cell::RefCell;
use std::time::Duration;

/// Largest number of points a script may draw (as for cutting contours).
const MAX_POINTS: usize = 1_000_000;
const MAX_MESSAGES: usize = 20;

/// Same helpers as LibLaserCut's bootstrap. `get` answers like VisiCut's
/// importer (values set by the script itself); `set` also starts a new
/// [`Segment`], as VisiCut runs the script once more for the job, where
/// `set` changes the laser parameters of all following lines.
const BOOTSTRAP: &str = r#"
var __settings = {};
function set(property, value) { __settings[property] = value; __set(String(property), value); }
function get(property) { var v = __settings[property]; return v === undefined ? null : v; }
function promptFloat(title, defaultValue) {
  var result = parseFloat(prompt(title, defaultValue.toString()));
  return isNaN(result) ? defaultValue : result;
}
"#;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Drawing {
    pub path: String,
    /// The drawing split at every `set`, in script order.
    pub segments: Vec<Segment>,
    /// `set` calls after the last line.
    pub after: Vec<Setting>,
    pub bounds: Option<[f64; 4]>,
    pub lines: usize,
    pub messages: Vec<String>,
}

/// Power and speed in percent after a `set`, everything the script set so
/// far; `None` keeps the value of the step.
pub type Setting = (Option<f64>, Option<f64>);

/// Lines between two calls of `set` (VisiCut's `VectorPartScriptInterface`:
/// every `set` is a property change in the job, which ends the polyline).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub path: String,
    /// One entry per `set` since the previous segment, in order.
    pub sets: Vec<Setting>,
    /// Continues the previous segment at its last point without a move (a
    /// `set` between two lines).
    pub joined: bool,
}

#[derive(Default)]
struct State {
    path: String,
    segments: Vec<Segment>,
    /// `set` calls since the last point; the next point starts a segment.
    pending: Vec<Setting>,
    split: bool,
    power: Option<f64>,
    speed: Option<f64>,
    last: Option<(f64, f64)>,
    bounds: Option<[f64; 4]>,
    started: bool,
    points: usize,
    max_points: usize,
    lines: usize,
    overflow: bool,
    messages: Vec<String>,
    dropped_messages: usize,
}

thread_local! {
    // Each script runs on its own worker thread, so this state is per run.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn run(source: String, limit: Duration) -> Result<Drawing, String> {
    run_with(source, limit, MAX_POINTS)
}

fn run_with(source: String, limit: Duration, max_points: usize) -> Result<Drawing, String> {
    use crate::import::worker::{Job, Output};
    match (Job::LaserScript { source, max_points }).run(limit)? {
        Output::Drawing(drawing) => Ok(drawing),
        _ => Err(tr("Skriptausführung ist unerwartet abgebrochen").into()),
    }
}

/// [`run`] on a thread of this process.
pub(crate) fn run_here(
    source: String,
    limit: Duration,
    max_points: usize,
) -> Result<Drawing, String> {
    script::run_isolated(limit, move |engine| execute(engine, &source, max_points))
}

fn execute(engine: &mut Engine, source: &str, max_points: usize) -> Result<Drawing, String> {
    STATE.with(|s| s.borrow_mut().max_points = max_points);
    register(&mut engine.context, "move", 2, js_move)?;
    register(&mut engine.context, "line", 2, js_line)?;
    register(&mut engine.context, "echo", 1, js_echo)?;
    register(&mut engine.context, "prompt", 2, js_prompt)?;
    register(&mut engine.context, "__set", 2, js_set)?;
    engine.eval(BOOTSTRAP)?;
    let result = engine.eval(source);
    let state = STATE.with(|s| std::mem::take(&mut *s.borrow_mut()));
    if state.overflow {
        return Err(tr!(
            "LaserScript zeichnet mehr als {count} Punkte; Import abgebrochen",
            count = max_points
        ));
    }
    result.map_err(|e| format!("LaserScript: {e}"))?;
    let mut messages = state.messages;
    if state.dropped_messages > 0 {
        messages.push(tr!(
            "LaserScript: {count} weitere Ausgaben ausgelassen",
            count = state.dropped_messages
        ));
    }
    Ok(Drawing {
        path: state.path,
        segments: state.segments,
        after: state.pending,
        bounds: state.bounds,
        lines: state.lines,
        messages,
    })
}

fn register(
    context: &mut Context,
    name: &str,
    length: usize,
    function: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> Result<(), String> {
    context
        .register_global_builtin_callable(
            JsString::from(name),
            length,
            NativeFunction::from_fn_ptr(function),
        )
        .map_err(|e| script::describe(&e))
}

fn coordinates(name: &str, args: &[JsValue], context: &mut Context) -> JsResult<(f64, f64)> {
    let x = args.get_or_undefined(0).to_number(context)?;
    let y = args.get_or_undefined(1).to_number(context)?;
    if !x.is_finite() || !y.is_finite() {
        return Err(JsNativeError::typ()
            .with_message(format!("{name} called with ({x},{y})"))
            .into());
    }
    Ok((x, y))
}

fn add_point(command: char, x: f64, y: f64) -> JsResult<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.points += 1;
        if state.points > state.max_points {
            state.overflow = true;
            return Err(JsNativeError::range()
                .with_message("too many points")
                .into());
        }
        if command == 'L' {
            state.lines += 1;
        }
        state.started = true;
        let [x0, y0, x1, y1] = state.bounds.unwrap_or([x, y, x, y]);
        state.bounds = Some([x0.min(x), y0.min(y), x1.max(x), y1.max(y)]);
        let separator = if state.path.is_empty() { "" } else { " " };
        let segment = format!("{separator}{command}{} {}", number(x), number(y));
        state.path.push_str(&segment);
        if state.segments.is_empty() || state.split {
            let sets = std::mem::take(&mut state.pending);
            state.segments.push(Segment {
                sets,
                ..Segment::default()
            });
            state.split = false;
        }
        let last = state.last;
        let current = state.segments.last_mut().unwrap();
        if current.path.is_empty()
            && command == 'L'
            && let Some((lx, ly)) = last
        {
            current.path = format!("M{} {}", number(lx), number(ly));
            current.joined = true;
        }
        let separator = if current.path.is_empty() { "" } else { " " };
        current
            .path
            .push_str(&format!("{separator}{command}{} {}", number(x), number(y)));
        state.last = Some((x, y));
        Ok(())
    })
}

fn js_move(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (x, y) = coordinates("Move", args, context)?;
    add_point('M', x, y)?;
    Ok(JsValue::undefined())
}

fn js_line(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (x, y) = coordinates("Line", args, context)?;
    // VisiCut starts a shape at the origin when it begins with a line.
    if !STATE.with(|s| s.borrow().started) {
        add_point('M', 0.0, 0.0)?;
    }
    add_point('L', x, y)?;
    Ok(JsValue::undefined())
}

/// `set(property, value)`: every call ends the current segment; power and
/// speed (numbers) apply to the following lines.
fn js_set(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let property = args
        .get_or_undefined(0)
        .to_string(context)?
        .to_std_string_escaped();
    let value = args
        .get_or_undefined(1)
        .to_number(context)
        .ok()
        .filter(|v| v.is_finite());
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        match property.as_str() {
            "power" if value.is_some() => state.power = value,
            "speed" if value.is_some() => state.speed = value,
            _ => {}
        }
        let setting = (state.power, state.speed);
        state.pending.push(setting);
        state.split = true;
    });
    Ok(JsValue::undefined())
}

fn message(text: String) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.messages.len() < MAX_MESSAGES {
            let text: String = text.chars().take(300).collect();
            state.messages.push(text);
        } else {
            state.dropped_messages += 1;
        }
    });
}

fn js_echo(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = args.get_or_undefined(0).to_string(context)?;
    message(tr!(
        "LaserScript-Ausgabe: {text}",
        text = text.to_std_string_escaped()
    ));
    Ok(JsValue::undefined())
}

/// VisiCut asks the user; the import uses the script's default answer.
fn js_prompt(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let title = args.get_or_undefined(0).to_string(context)?;
    let default = args.get_or_undefined(1);
    let answer = if default.is_null_or_undefined() {
        js_string!("")
    } else {
        default.to_string(context)?
    };
    message(tr!(
        "LaserScript-Eingabe „{title}“: Vorgabewert „{value}“ verwendet",
        title = title.to_std_string_escaped(),
        value = answer.to_std_string_escaped()
    ));
    Ok(answer.into())
}

pub fn number(value: f64) -> String {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 {
        "0".into()
    } else {
        format!("{rounded}")
    }
}

/// SVG in millimetres; coordinates stay those of the script, so the drawing
/// keeps its position relative to the laser's origin.
pub fn to_svg(drawing: &Drawing) -> Result<(String, Vec<String>), String> {
    let Some([x0, y0, x1, y1]) = drawing.bounds.filter(|_| drawing.lines > 0) else {
        return Err(tr("LaserScript hat keine Linien gezeichnet").into());
    };
    let mut warnings = Vec::new();
    if x0 < 0.0 || y0 < 0.0 {
        warnings.push(
            tr("LaserScript zeichnet links oder oberhalb des Nullpunkts; \
                die Grafik beginnt deshalb dort")
            .into(),
        );
    }
    let left = x0.min(0.0);
    let top = y0.min(0.0);
    let width = (x1 - left).max(1.0);
    let height = (y1 - top).max(1.0);
    // One path per segment; the id carries what the job needs (see
    // `geometry::laser_script`): `laserscript-<n>`, then `_e<power>:<speed>`
    // per `set` before it (`n`: not set), `_j` when joined and on the last
    // path `_t<power>:<speed>` per `set` after the last line.
    let setting = |(power, speed): &Setting| {
        let value = |v: &Option<f64>| v.map_or("n".to_string(), number);
        format!("{}:{}", value(power), value(speed))
    };
    let mut paths = String::new();
    let count = drawing.segments.len();
    for (index, segment) in drawing.segments.iter().enumerate() {
        let mut id = format!("laserscript-{index}");
        for set in &segment.sets {
            id += &format!("_e{}", setting(set));
        }
        if segment.joined {
            id += "_j";
        }
        if index + 1 == count {
            for set in &drawing.after {
                id += &format!("_t{}", setting(set));
            }
        }
        paths += &format!(
            "<path id=\"{id}\" inkscape:label=\"LaserScript\" d=\"{d}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"0.1\"/>\n",
            d = segment.path
        );
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{l} {t} {w} {h}\">\n{paths}</svg>\n",
        w = number(width),
        h = number(height),
        l = number(left),
        t = number(top),
    );
    Ok((svg, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_paths_in_millimetres() {
        let drawing = run(
            "set('power', 50); move(1, 2); line(3.5, 2); line(get('power') / 10, 4); echo('ok');"
                .into(),
            script::TIME_LIMIT,
        )
        .unwrap();
        assert_eq!(drawing.path, "M1 2 L3.5 2 L5 4");
        assert_eq!(drawing.messages, ["LaserScript-Ausgabe: ok"]);
    }

    #[test]
    fn set_splits_the_drawing_and_keeps_every_call() {
        // Power before drawing, a speed change between two lines (joined),
        // a focus change (no value for the job, still a split) and one more
        // power change after the last line.
        let drawing = run(
            "set('power', 0); set('speed', 10); move(0, 0); line(1, 0); \
             set('speed', 20); line(2, 0); set('focus', 3); move(5, 5); line(6, 5); \
             set('power', 90);"
                .into(),
            script::TIME_LIMIT,
        )
        .unwrap();
        let segments: Vec<_> = drawing
            .segments
            .iter()
            .map(|s| (s.path.as_str(), s.sets.clone(), s.joined))
            .collect();
        assert_eq!(
            segments,
            [
                (
                    "M0 0 L1 0",
                    vec![(Some(0.0), None), (Some(0.0), Some(10.0))],
                    false
                ),
                ("M1 0 L2 0", vec![(Some(0.0), Some(20.0))], true),
                ("M5 5 L6 5", vec![(Some(0.0), Some(20.0))], false),
            ]
        );
        assert_eq!(drawing.after, [(Some(90.0), Some(20.0))]);
        let (svg, _) = to_svg(&drawing).unwrap();
        assert!(svg.contains(r#"id="laserscript-0_e0:n_e0:10""#), "{svg}");
        assert!(svg.contains(r#"id="laserscript-1_e0:20_j""#), "{svg}");
        assert!(svg.contains(r#"id="laserscript-2_e0:20_t90:20""#), "{svg}");
    }

    #[test]
    fn line_without_move_starts_at_origin() {
        let drawing = run("line(10, 0)".into(), script::TIME_LIMIT).unwrap();
        assert_eq!(drawing.path, "M0 0 L10 0");
    }

    #[test]
    fn prompts_use_defaults() {
        let drawing = run(
            "var s = promptFloat('Größe', 7); move(0, 0); line(s, 0);".into(),
            script::TIME_LIMIT,
        )
        .unwrap();
        assert_eq!(drawing.path, "M0 0 L7 0");
        assert!(drawing.messages[0].contains("Vorgabewert „7“"));
    }

    #[test]
    fn rejects_invalid_coordinates_and_errors() {
        assert!(run("move('a', 1)".into(), script::TIME_LIMIT).is_err());
        let error = run("move(0, 0); line(1, ".into(), script::TIME_LIMIT).unwrap_err();
        assert!(error.starts_with("LaserScript: Skriptfehler"), "{error}");
    }

    #[test]
    fn limits_output_size() {
        let error = run_with(
            "for (var i = 0; i < 2000; i++) { try { line(i % 100, 1); } catch (e) {} }".into(),
            script::TIME_LIMIT,
            1000,
        )
        .unwrap_err();
        assert!(error.contains("Punkte"), "{error}");
    }
}
