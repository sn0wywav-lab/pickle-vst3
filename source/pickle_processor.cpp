#include "pickle_processor.h"
#include "pickle_controller.h"
#include <algorithm>
#include <cstring>

namespace Pickle {

const Steinberg::FUID PickleProcessor::uid(0x12345678, 0x12345678, 0x12345678, 0x12345678);

PickleProcessor::PickleProcessor()
{
    delayBuffer.resize(maxDelaySize, 0.0f);
    setControllerClass(PickleController::uid);
}

Steinberg::tresult PLUGIN_API PickleProcessor::initialize(Steinberg::FUnknown* context)
{
    //---always call this------------------------------------
    Steinberg::tresult result = AudioEffect::initialize(context);
    if (result != Steinberg::kResultOk)
        return result;

    //---add audio input/output buses-----------------------
    addAudioInput(STR16("AudioInput"), Steinberg::Vst::SpeakerArr::kStereo);
    addAudioOutput(STR16("AudioOutput"), Steinberg::Vst::SpeakerArr::kStereo);

    //---add event input/output-----------------------------
    addEventInput(STR16("EventInput"), 1);

    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleProcessor::setBusArrangements(
    Steinberg::Vst::SpeakerArrangement* inputs,
    Steinberg::int32 numIns,
    Steinberg::Vst::SpeakerArrangement* outputs,
    Steinberg::int32 numOuts)
{
    // we only support stereo in/out
    if (numIns == 1 && numOuts == 1 && inputs[0] == Steinberg::Vst::SpeakerArr::kStereo &&
        outputs[0] == Steinberg::Vst::SpeakerArr::kStereo)
    {
        return AudioEffect::setBusArrangements(inputs, numIns, outputs, numOuts);
    }
    return Steinberg::kResultFalse;
}

Steinberg::tresult PLUGIN_API PickleProcessor::setActive(Steinberg::TBool state)
{
    if (state)
    {
        // reset delay buffer
        std::fill(delayBuffer.begin(), delayBuffer.end(), 0.0f);
        delayBufferPos = 0;
    }
    return AudioEffect::setActive(state);
}

Steinberg::tresult PLUGIN_API PickleProcessor::process(Steinberg::Vst::ProcessData& data)
{
    //---1. Read input parameters coming from host-----------------
    if (data.inputParameterChanges)
    {
        Steinberg::int32 numParamsChanged = data.inputParameterChanges->getParameterCount();
        for (Steinberg::int32 i = 0; i < numParamsChanged; i++)
        {
            Steinberg::Vst::IParamValueQueue* paramQueue =
                data.inputParameterChanges->getParameterData(i);
            if (paramQueue)
            {
                Steinberg::Vst::ParamID id = paramQueue->getParameterId();
                Steinberg::int32 numPoints = paramQueue->getPointCount();
                Steinberg::int32 sampleOffset;
                Steinberg::Vst::ParamValue value;
                if (paramQueue->getPoint(numPoints - 1, sampleOffset, value) ==
                    Steinberg::kResultTrue)
                {
                    switch (id)
                    {
                    case kPickleBrine:
                        fPickleBrine = value;
                        break;
                    case kPickleCrunch:
                        fPickleCrunch = value;
                        break;
                    case kPickleSpice:
                        fPickleSpice = value;
                        break;
                    case kPickleFerment:
                        fPickleFerment = value;
                        break;
                    case kPickleJuice:
                        fPickleJuice = value;
                        break;
                    case kPickleProcessor:
                        fPickleProcessor = value;
                        break;
                    case kPickleCrunchNew:
                        fPickleCrunchNew = value;
                        break;
                    case kPickleBitcrush:
                        fPickleBitcrush = value;
                        // Update bitcrush rate based on amount
                        bitcrushRate = static_cast<int>(44100 * (1.0f - value * 0.9f));
                        break;
                    }
                }
            }
        }
    }

    //---2. Process audio-----------------------------------------------
    if (data.numSamples > 0)
    {
        processAudio(data);
    }

    //---3. Write output parameter changes--------------------------------
    // (none for this simple effect)

    return Steinberg::kResultOk;
}

void PickleProcessor::processAudio(Steinberg::Vst::ProcessData& data)
{
    Steinberg::Vst::AudioBusBuffers* input = data.inputs;
    Steinberg::Vst::AudioBusBuffers* output = data.outputs;
    
    if (!input || !output || !input->channelBuffers32[0] || !output->channelBuffers32[0])
        return;

    Steinberg::int32 numChannels = input->numChannels;
    Steinberg::int32 sampleFrames = data.numSamples;

    for (Steinberg::int32 i = 0; i < sampleFrames; i++)
    {
        for (Steinberg::int32 chan = 0; chan < numChannels; chan++)
        {
            float inputSample = input->channelBuffers32[chan][i];
            float outputSample = inputSample;

            // Apply pickle crunch (distortion)
            outputSample = pickleDistortion(outputSample, fPickleCrunch);
            
            // Apply new pickle crunch with bitcrush
            outputSample = pickleCrunchNew(outputSample, fPickleCrunchNew, fPickleBitcrush);
            
            // Apply pickle processor (multiband compression)
            outputSample = pickleProcessorMultiband(outputSample, fPickleProcessor);

            // Apply pickle spice (high-frequency enhancement)
            outputSample = pickleSpiceFilter(outputSample, fPickleSpice);

            // Apply pickle ferment (delay/feedback)
            if (fPickleFerment > 0.0f)
            {
                float delayTime = fPickleFerment * maxDelaySize * 0.5f;
                Steinberg::int32 delaySamples = static_cast<Steinberg::int32>(delayTime);
                Steinberg::int32 readPos = (delayBufferPos - delaySamples + maxDelaySize) % maxDelaySize;
                
                float delayedSample = delayBuffer[readPos];
                outputSample = outputSample * (1.0f - fPickleFerment * 0.5f) + delayedSample * fPickleFerment * 0.5f;
                
                delayBuffer[delayBufferPos] = outputSample;
                delayBufferPos = (delayBufferPos + 1) % maxDelaySize;
            }

            // Apply pickle brine (wet/dry mix)
            outputSample = inputSample * (1.0f - fPickleBrine) + outputSample * fPickleBrine;

            // Apply pickle juice (output gain)
            outputSample *= fPickleJuice;

            // Soft clipping to prevent harsh digital distortion
            if (outputSample > 0.95f)
                outputSample = 0.95f + (outputSample - 0.95f) * 0.1f;
            else if (outputSample < -0.95f)
                outputSample = -0.95f + (outputSample + 0.95f) * 0.1f;

            output->channelBuffers32[chan][i] = outputSample;
        }
    }
}

float PickleProcessor::pickleDistortion(float input, float amount)
{
    if (amount <= 0.0f)
        return input;

    // Smooth distortion curve that sounds like a "crunchy" pickle
    float gain = 1.0f + amount * 3.0f;
    float distorted = input * gain;
    
    // Soft clipping with asymmetric curve for harmonic richness
    if (distorted > 0.0f)
    {
        distorted = tanhf(distorted * 0.7f) * 1.3f;
    }
    else
    {
        distorted = tanhf(distorted * 0.9f) * 1.1f;
    }
    
    return distorted;
}

float PickleProcessor::pickleSpiceFilter(float input, float amount)
{
    if (amount <= 0.0f)
        return input;

    // Simple high-frequency enhancement (spice)
    static float prevSample = 0.0f;
    float highFreq = input - prevSample;
    float enhanced = input + highFreq * amount * 0.3f;
    prevSample = input;
    
    return enhanced;
}

float PickleProcessor::pickleProcessorMultiband(float input, float amount)
{
    if (amount <= 0.0f)
        return input;

    // Simple 3-band split using cascaded filters
    static float lowState = 0.0f, midState = 0.0f, highState = 0.0f;
    
    // Low band (below 300Hz)
    float lowFreq = 300.0f / 44100.0f;
    lowState += (input - lowState) * lowFreq;
    float lowBand = lowState;
    
    // Mid band (300Hz - 3kHz)
    float midFreq = 3000.0f / 44100.0f;
    float midInput = input - lowBand;
    midState += (midInput - midState) * midFreq;
    float midBand = midState - lowState;
    
    // High band (above 3kHz)
    float highBand = input - lowBand - midBand;
    
    // Apply compression to each band
    float compressionAmount = amount * 0.5f; // Reduce intensity for subtlety
    
    // Low band compression
    float lowAbs = std::abs(lowBand);
    multiband.lowEnvelope += (lowAbs - multiband.lowEnvelope) * (lowAbs > multiband.lowEnvelope ? 0.01f : 0.999f);
    float lowGainReduction = 1.0f / (1.0f + multiband.lowEnvelope * compressionAmount * 2.0f);
    multiband.lowGain += (lowGainReduction - multiband.lowGain) * 0.1f;
    float processedLow = lowBand * multiband.lowGain;
    
    // Mid band compression (upward compression for forwardness)
    float midAbs = std::abs(midBand);
    multiband.midEnvelope += (midAbs - multiband.midEnvelope) * (midAbs > multiband.midEnvelope ? 0.01f : 0.999f);
    float midGainIncrease = 1.0f + multiband.midEnvelope * compressionAmount;
    multiband.midGain += (midGainIncrease - multiband.midGain) * 0.1f;
    float processedMid = midBand * multiband.midGain;
    
    // High band compression
    float highAbs = std::abs(highBand);
    multiband.highEnvelope += (highAbs - multiband.highEnvelope) * (highAbs > multiband.highEnvelope ? 0.01f : 0.999f);
    float highGainReduction = 1.0f / (1.0f + multiband.highEnvelope * compressionAmount * 1.5f);
    multiband.highGain += (highGainReduction - multiband.highGain) * 0.1f;
    float processedHigh = highBand * multiband.highGain;
    
    // Mix processed and original based on amount
    return input * (1.0f - amount) + (processedLow + processedMid + processedHigh) * amount;
}

float PickleProcessor::pickleCrunchNew(float input, float crunchAmount, float bitcrushAmount)
{
    if (crunchAmount <= 0.0f && bitcrushAmount <= 0.0f)
        return input;

    float output = input;
    
    // Apply subtle crunch distortion focused on low and high frequencies
    if (crunchAmount > 0.0f)
    {
        // Split into low and high frequencies
        static float lowFilter = 0.0f;
        float cutoff = 500.0f / 44100.0f;
        lowFilter += (output - lowFilter) * cutoff;
        float lowFreq = lowFilter;
        float highFreq = output - lowFreq;
        
        // Apply gentle saturation to lows
        float lowDistorted = lowFreq * (1.0f + crunchAmount * 0.3f);
        lowDistorted = std::tanh(lowDistorted * 1.2f) * 0.8f;
        
        // Apply gentle saturation to highs
        float highDistorted = highFreq * (1.0f + crunchAmount * 0.2f);
        highDistorted = std::tanh(highDistorted * 1.1f) * 0.9f;
        
        output = lowDistorted + highDistorted;
    }
    
    // Apply bitcrush effect
    if (bitcrushAmount > 0.0f)
    {
        bitcrushPhase += 1.0f;
        if (bitcrushPhase >= bitcrushRate)
        {
            bitcrushPhase -= bitcrushRate;
        }
        
        // Reduce bit depth
        int bitDepth = static_cast<int>(16 - bitcrushAmount * 12); // 16 to 4 bits
        float scale = std::pow(2.0f, static_cast<float>(bitDepth));
        output = std::floor(output * scale) / scale;
        
        // Add subtle sample rate reduction
        if (static_cast<int>(bitcrushPhase) % static_cast<int>(1 + bitcrushAmount * 3) == 0)
        {
            output = output * 0.9f + (output - output) * 0.1f; // Hold sample
        }
    }
    
    return output;
}

Steinberg::tresult PLUGIN_API PickleProcessor::setState(Steinberg::IBStream* state)
{
    if (!state)
        return Steinberg::kResultFalse;

    // Save current state
    Steinberg::int32 saved = 0;
    if (state->write(&fPickleBrine, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleCrunch, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleSpice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleFerment, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleJuice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleProcessor, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleCrunchNew, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;
    if (state->write(&fPickleBitcrush, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    saved++;

    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleProcessor::getState(Steinberg::IBStream* state)
{
    if (!state)
        return Steinberg::kResultFalse;

    // Load state
    if (state->read(&fPickleBrine, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleCrunch, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleSpice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleFerment, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleJuice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleProcessor, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleCrunchNew, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&fPickleBitcrush, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;

    return Steinberg::kResultOk;
}

Steinberg::FUnknown* PickleProcessor::createInstance(void* context)
{
    return static_cast<Steinberg::Vst::IAudioProcessor*>(new PickleProcessor());
}

} // namespace Pickle
