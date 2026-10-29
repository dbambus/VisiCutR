//! Compact chronological motion plan. Repeated passes share their geometry.
use crate::project::Operation;
use serde::Serialize;

pub const TRAVEL_MM_S: f64 = 338.677;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub enum MotionKind {
    Travel,
    Cut,
    Mark,
    Raster,
    Dwell,
}

#[derive(Serialize)]
pub struct Motion {
    pub kind: MotionKind,
    pub from_mm: [f64; 2],
    pub to_mm: [f64; 2],
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Serialize)]
pub struct Program {
    pub operation: Operation,
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    pub duration_seconds: f64,
    pub motions: Vec<Motion>,
    pub raster_preview_png: Vec<u8>,
    pub raster_bounds_mm: Option<[f64; 4]>,
    #[serde(skip)]
    initialized: bool,
}

impl Program {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            start_mm: [0.0; 2],
            end_mm: [0.0; 2],
            duration_seconds: 0.0,
            motions: Vec::new(),
            raster_preview_png: Vec::new(),
            raster_bounds_mm: None,
            initialized: false,
        }
    }

    pub fn travel(&mut self, to: [f64; 2]) {
        if !self.initialized {
            self.start_mm = to;
            self.end_mm = to;
            self.initialized = true;
        } else {
            self.line(MotionKind::Travel, to, TRAVEL_MM_S);
        }
    }

    pub fn line(&mut self, kind: MotionKind, to: [f64; 2], speed: f64) {
        self.push(kind, to, distance(self.end_mm, to) / speed);
    }

    pub fn started(&self) -> bool {
        self.initialized
    }

    /// Travel with a precomputed duration (acceleration-aware estimates).
    pub fn travel_timed(&mut self, to: [f64; 2], seconds: f64) {
        if !self.initialized {
            self.travel(to);
        } else {
            self.push(MotionKind::Travel, to, seconds);
        }
    }

    pub fn line_timed(&mut self, kind: MotionKind, to: [f64; 2], seconds: f64) {
        self.push(kind, to, seconds);
    }

    pub fn dwell(&mut self, seconds: f64) {
        self.push(MotionKind::Dwell, self.end_mm, seconds);
    }

    fn push(&mut self, kind: MotionKind, to: [f64; 2], seconds: f64) {
        if seconds <= 0.0 {
            return;
        }
        let start_seconds = self.duration_seconds;
        self.duration_seconds += seconds;
        self.motions.push(Motion {
            kind,
            from_mm: self.end_mm,
            to_mm: to,
            start_seconds,
            end_seconds: self.duration_seconds,
        });
        self.end_mm = to;
    }
}

#[derive(Serialize)]
pub struct Run {
    pub program_index: usize,
    pub pass: u32,
    pub from_mm: [f64; 2],
    pub start_seconds: f64,
    pub entry_end_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Default, Serialize)]
pub struct Timeline {
    pub programs: Vec<Program>,
    pub runs: Vec<Run>,
    pub duration_seconds: f64,
    /// Start of the next program when it begins a new device job.
    #[serde(skip)]
    job_start: Option<[f64; 2]>,
}

impl Timeline {
    /// The next program begins a new device job, which the machine starts
    /// from its home position.
    pub fn start_job(&mut self, home: [f64; 2]) {
        self.job_start = Some(home);
    }

    /// Head position before the next program.
    pub fn position(&self) -> [f64; 2] {
        self.job_start
            .or(self.programs.last().map(|p| p.end_mm))
            .unwrap_or([0.0; 2])
    }

    pub fn append(&mut self, program: Program, passes: u32) -> f64 {
        let start = self.duration_seconds;
        let mut from = self.position();
        self.job_start = None;
        for pass in 1..=passes {
            let entry_end_seconds =
                self.duration_seconds + distance(from, program.start_mm) / TRAVEL_MM_S;
            let end_seconds = entry_end_seconds + program.duration_seconds;
            self.runs.push(Run {
                program_index: self.programs.len(),
                pass,
                from_mm: from,
                start_seconds: self.duration_seconds,
                entry_end_seconds,
                end_seconds,
            });
            self.duration_seconds = end_seconds;
            from = program.end_mm;
        }
        self.programs.push(program);
        self.duration_seconds - start
    }
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_without_duplicating_geometry_and_keeps_head_continuous() {
        let mut program = Program::new(Operation::Engrave);
        program.travel([10.0, 10.0]);
        program.dwell(0.1);
        program.line(MotionKind::Raster, [30.0, 10.0], 100.0);
        let mut timeline = Timeline::default();
        timeline.append(program, 3);
        let mut cut = Program::new(Operation::Cut);
        cut.travel([40.0, 20.0]);
        cut.line(MotionKind::Cut, [50.0, 20.0], 10.0);
        timeline.append(cut, 1);
        assert_eq!(timeline.programs.len(), 2);
        assert_eq!(timeline.programs[0].motions.len(), 2);
        assert_eq!(timeline.runs.len(), 4);
        assert_eq!(timeline.runs[0].from_mm, [0.0, 0.0]);
        for run in &timeline.runs[1..] {
            assert_eq!(run.from_mm, [30.0, 10.0]);
        }
        for pair in timeline.runs.windows(2) {
            assert_eq!(pair[0].end_seconds, pair[1].start_seconds);
        }
        assert_eq!(
            timeline.duration_seconds,
            timeline.runs.last().unwrap().end_seconds
        );
        assert_eq!(timeline.runs[2].pass, 3);
    }

    #[test]
    fn a_new_job_starts_at_home_and_continues_from_there() {
        let mut timeline = Timeline::default();
        timeline.start_job([1000.0, 0.0]);
        assert_eq!(timeline.position(), [1000.0, 0.0]);
        let mut first = Program::new(Operation::Cut);
        first.travel([700.0, 400.0]);
        first.line(MotionKind::Cut, [710.0, 400.0], 10.0);
        let seconds = timeline.append(first, 1);
        assert_eq!(timeline.runs[0].from_mm, [1000.0, 0.0]);
        assert!((seconds - 500.0 / TRAVEL_MM_S - 1.0).abs() < 1e-9);
        // Same job: the next program follows on from the previous end.
        let mut second = Program::new(Operation::Cut);
        second.travel([10.0, 10.0]);
        timeline.append(second, 1);
        assert_eq!(timeline.runs[1].from_mm, [710.0, 400.0]);
        timeline.start_job([1000.0, 0.0]);
        let mut third = Program::new(Operation::Engrave);
        third.travel([10.0, 10.0]);
        timeline.append(third, 1);
        assert_eq!(timeline.runs[2].from_mm, [1000.0, 0.0]);
    }
}
