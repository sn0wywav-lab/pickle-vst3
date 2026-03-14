#pragma once

#include "public.sdk/source/vst/vstaudioeffect.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/base/funknown.h"
#include <cmath>
#include <vector>

namespace Pickle {

class PickleProcessor : public Steinberg::Vst::AudioEffect
{
public:
    PickleProcessor();
    ~PickleProcessor() override = default;

    //--- AudioEffect overrides -------------------------------------------------
    Steinberg::tresult PLUGIN_API initialize(Steinberg::FUnknown* context) override;
    Steinberg::tresult PLUGIN_API setBusArrangements(Steinberg::Vst::SpeakerArrangement* inputs,
        Steinberg::int32 numIns, Steinberg::Vst::SpeakerArrangement* outputs,
        Steinberg::int32 numOuts) override;
    Steinberg::tresult PLUGIN_API setActive(Steinberg::TBool state) override;
    Steinberg::tresult PLUGIN_API process(Steinberg::Vst::ProcessData& data) override;
    Steinberg::tresult PLUGIN_API setState(Steinberg::IBStream* state) override;
    Steinberg::tresult PLUGIN_API getState(Steinberg::IBStream* state) override;

    //--- Static methods --------------------------------------------------------
    static Steinberg::FUnknown* createInstance(void* context);

    //--- UID ------------------------------------------------------------------
    static const Steinberg::FUID uid;

private:
    // Audio processing parameters
    enum Parameters
    {
        kPickleBrine = 0,    // Wet/Dry mix
        kPickleCrunch,       // Distortion amount
        kPickleSpice,        // High-frequency enhancement
        kPickleFerment,      // Delay/feedback
        kPickleJuice,        // Output gain
        kPickleProcessor,    // Multiband compression (OTT-style)
        kPickleCrunchNew,    // New crunch distortion
        kPickleBitcrush,     // Bitcrush sub-parameter
        kNumParameters
    };

    // Parameter values
    float fPickleBrine{0.5f};
    float fPickleCrunch{0.0f};
    float fPickleSpice{0.5f};
    float fPickleFerment{0.0f};
    float fPickleJuice{1.0f};
    float fPickleProcessor{0.0f};
    float fPickleCrunchNew{0.0f};
    float fPickleBitcrush{0.0f};

    // Delay buffer for fermentation effect
    std::vector<float> delayBuffer;
    Steinberg::int32 delayBufferPos{0};
    const Steinberg::int32 maxDelaySize{44100}; // 1 second at 44.1kHz

    // Multiband compression for Pickle Processor
    struct MultibandCompressor
    {
        float lowGain{1.0f}, midGain{1.0f}, highGain{1.0f};
        float lowEnvelope{0.0f}, midEnvelope{0.0f}, highEnvelope{0.0f};
        const float releaseCoeff{0.999f};
        const float attackCoeff{0.01f};
    } multiband;

    // Bitcrush state
    float bitcrushPhase{0.0f};
    int bitcrushRate{44100};

    // Helper functions
    float pickleDistortion(float input, float amount);
    float pickleSpiceFilter(float input, float amount);
    float pickleProcessorMultiband(float input, float amount);
    float pickleCrunchNew(float input, float crunchAmount, float bitcrushAmount);
    void processAudio(Steinberg::Vst::ProcessData& data);
};

} // namespace Pickle
