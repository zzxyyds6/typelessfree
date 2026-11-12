//! 麦克风录音采集服务。
//! cpal::Stream 在部分平台（如 macOS）不可跨线程移动，
//! 因此音频流只能在专用线程中创建与销毁，外部通过命令通道控制。

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

enum AudioCmd {
    Start(mpsc::Sender<Result<(), String>>),
    Stop(mpsc::Sender<Result<Vec<f32>, String>>),
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
        Ok(crate::transcribe::resample_to_16k(&buf, self.sample_rate))
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
