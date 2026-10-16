// Microphone recording for dictation (Qt Multimedia): the default input,
// as float samples, kept in memory until stopped.
#pragma once

#include <QtCore/QIODevice>
#include <QtCore/QString>
#include <QtCore/QtGlobal>
#include <QtMultimedia/QAudioDevice>
#include <QtMultimedia/QAudioFormat>
#include <QtMultimedia/QAudioSource>
#include <QtMultimedia/QMediaDevices>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <memory>
#include <vector>

#include "rust/cxx.h"

namespace harlo {

// Receives the audio source's bytes and keeps them as floats.
class SampleCollector final : public QIODevice
{
public:
    explicit SampleCollector(const QAudioFormat &format) : m_format(format) { }
    std::vector<float> samples;
    // Loudest sample of the latest chunk (0–1), for a level meter.
    float level = 0;

protected:
    qint64 readData(char *, qint64) override { return 0; }
    qint64 writeData(const char *data, qint64 len) override
    {
        const int bytes = m_format.bytesPerSample();
        if (bytes <= 0)
            return len;
        float peak = 0;
        for (qint64 i = 0; i + bytes <= len; i += bytes) {
            const float v = m_format.normalizedSampleValue(data + i);
            samples.push_back(v);
            peak = std::max(peak, std::fabs(v));
        }
        level = peak;
        return len;
    }

private:
    QAudioFormat m_format;
};

struct Recording
{
    std::unique_ptr<SampleCollector> collector;
    std::unique_ptr<QAudioSource> source;
    int rate = 0;
    int channels = 0;
};

inline Recording &recording()
{
    static Recording r;
    return r;
}

inline bool audio_failed(const QAudioSource &source)
{
#if QT_VERSION >= QT_VERSION_CHECK(6, 7, 0)
    return source.error() != QtAudio::NoError;
#else
    return source.error() != QAudio::NoError;
#endif
}

inline void audio_cancel()
{
    Recording &r = recording();
    if (r.source)
        r.source->stop();
    r.source.reset();
    r.collector.reset();
}

// Starts recording from the default microphone. Returns an error, or "".
inline QString audio_start()
{
    audio_cancel();
    const QAudioDevice device = QMediaDevices::defaultAudioInput();
    if (device.isNull())
        return QStringLiteral("No microphone found.");
    // 16 kHz mono floats if the device takes them; else its own format
    // (converted later).
    QAudioFormat format;
    format.setSampleRate(16000);
    format.setChannelCount(1);
    format.setSampleFormat(QAudioFormat::Float);
    if (!device.isFormatSupported(format))
        format = device.preferredFormat();
    Recording &r = recording();
    r.collector = std::make_unique<SampleCollector>(format);
    r.collector->open(QIODevice::WriteOnly);
    r.source = std::make_unique<QAudioSource>(device, format);
    r.rate = format.sampleRate();
    r.channels = format.channelCount();
    r.source->start(r.collector.get());
    if (audio_failed(*r.source)) {
        audio_cancel();
        return QStringLiteral("Could not open the microphone (%1).").arg(device.description());
    }
    return {};
}

// Stops; the recorded samples (interleaved), their rate and channels.
inline rust::Vec<float> audio_stop(std::int32_t &rate, std::int32_t &channels)
{
    rust::Vec<float> out;
    Recording &r = recording();
    if (!r.source || !r.collector)
        return out;
    r.source->stop();
    rate = r.rate;
    channels = r.channels;
    out.reserve(r.collector->samples.size());
    for (float s : r.collector->samples)
        out.push_back(s);
    audio_cancel();
    return out;
}

inline float audio_level()
{
    const Recording &r = recording();
    return r.collector ? r.collector->level : 0.f;
}

} // namespace harlo
