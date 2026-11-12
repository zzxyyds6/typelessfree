//! 麦克风录音采集服务。
//! cpal::Stream 在部分平台（如 macOS）不可跨线程移动，
//! 因此音频流只能在专用线程中创建与销毁，外部通过命令通道控制。

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

enum AudioCmd {
    Start(mpsc::Sender<Result<(), String>>),
    Stop(mpsc::Sender<Result<Vec<f32>, String>>),
    /// 快照当前录音缓冲（用于实时 partial 识别与音量条），不停止录音。
    Snapshot(mpsc::Sender<Result<AudioSnapshot, String>>),
}

/// 录音快照：是否在录音、16kHz 重采样样本、归一化音量（0~1）。
pub struct AudioSnapshot {
    pub recording: bool,
    pub samples: Vec<f32>,
    pub level: f32,
}

/// 音频服务句柄：可在任意线程调用（内部命令走通道）。
pub struct AudioHandle {
    tx: Mutex<mpsc::Sender<AudioCmd>>,
}

impl AudioHandle {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCmd>();
        std::thread::spawn(move || {
            let mut recorder = AudioRecorder::new();
            for cmd in rx {
                match cmd {
                    AudioCmd::Start(reply) => {
                        let _ = reply.send(recorder.start());
                    }
                    AudioCmd::Stop(reply) => {
                        let _ = reply.send(recorder.stop());
                    }
                    AudioCmd::Snapshot(reply) => {
                        let _ = reply.send(recorder.snapshot());
                    }
                }
            }
        });
        Self {
            tx: Mutex::new(tx),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .lock()
            .unwrap()
            .send(AudioCmd::Start(reply_tx))
            .map_err(|_| "音频服务已停止".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "音频服务无响应".to_string())?
    }

    /// 停止录音并返回 16kHz 单声道样本。
    pub fn stop(&self) -> Result<Vec<f32>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .lock()
            .unwrap()
            .send(AudioCmd::Stop(reply_tx))
            .map_err(|_| "音频服务已停止".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "音频服务无响应".to_string())?
    }

    /// 获取当前录音快照（不中断录音）。
    pub fn snapshot(&self) -> Result<AudioSnapshot, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .lock()
            .unwrap()
            .send(AudioCmd::Snapshot(reply_tx))
            .map_err(|_| "音频服务已停止".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "音频服务无响应".to_string())?
    }
}

struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl AudioRecorder {
    fn new() -> Self {
        Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 16000,
        }
    }

    fn start(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Err("已在录音中".to_string());
        }
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "未找到麦克风输入设备".to_string())?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("读取麦克风配置失败: {e}"))?;
        self.sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let buffer = Arc::clone(&self.buffer);
        buffer.lock().unwrap().clear();
        let err_fn = |e| eprintln!("音频流错误: {e}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    buffer.lock().unwrap().extend(mono(data, channels));
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                    buffer.lock().unwrap().extend(mono(&f, channels));
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _| {
                    let f: Vec<f32> = data
                        .iter()
                        .map(|s| (*s as f32 - 32768.0) / 32768.0)
                        .collect();
                    buffer.lock().unwrap().extend(mono(&f, channels));
                },
                err_fn,
                None,
            ),
            other => {
                return Err(format!("不支持的采样格式: {other:?}"));
            }
        }
        .map_err(|e| format!("打开录音流失败: {e}"))?;

        stream.play().map_err(|e| format!("启动录音失败: {e}"))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> Result<Vec<f32>, String> {
        let stream = self.stream.take().ok_or_else(|| "当前未在录音".to_string())?;
        drop(stream); // 关闭音频流
        let buf = self.buffer.lock().unwrap().clone();
        self.buffer.lock().unwrap().clear();
        Ok(crate::transcribe::resample_to_16k(&buf, self.sample_rate))
    }

    fn snapshot(&self) -> Result<AudioSnapshot, String> {
        if self.stream.is_none() {
            return Ok(AudioSnapshot {
                recording: false,
                samples: Vec::new(),
                level: 0.0,
            });
        }
        let buf = self.buffer.lock().unwrap().clone();
        let samples = crate::transcribe::resample_to_16k(&buf, self.sample_rate);

        // 计算 RMS 音量并做增益与截断，方便前端显示音量条
        let rms = if samples.is_empty() {
            0.0
        } else {
            (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
        };
        let level = (rms * 6.0).clamp(0.0, 1.0);

        Ok(AudioSnapshot {
            recording: true,
            samples,
            level,
        })
    }
}

/// 多声道混为单声道。
fn mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        data.to_vec()
    } else {
        data.chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    }
}