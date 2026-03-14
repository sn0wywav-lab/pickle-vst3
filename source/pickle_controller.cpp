#include "pickle_controller.h"
#include "pickle_processor.h"
#include "pickle_view.h"
#include "pluginterfaces/vst/ivstparameterchanges.h"

namespace Pickle {

const Steinberg::FUID PickleController::uid(0x12345678, 0x12345678, 0x12345678, 0x12345678);

Steinberg::tresult PLUGIN_API PickleController::initialize(Steinberg::FUnknown* context)
{
    Steinberg::tresult result = EditController::initialize(context);
    if (result != Steinberg::kResultOk)
        return result;

    //---Pickle Brine (Wet/Dry Mix)-----------------------------------------
    Steinberg::Vst::Parameter* param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleBrine,
        STR16("Pickle Brine"),
        STR16("Brine"),
        0,
        0,
        1.0,
        0.5,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Mix between dry and pickled sound"));
    parameters.addParameter(param);

    //---Pickle Crunch (Distortion)-----------------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleCrunch,
        STR16("Pickle Crunch"),
        STR16("Crunch"),
        1,
        0,
        1.0,
        0.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Distortion and crunch intensity"));
    parameters.addParameter(param);

    //---Pickle Spice (High-Freq Enhancement)-------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleSpice,
        STR16("Pickle Spice"),
        STR16("Spice"),
        2,
        0,
        1.0,
        0.5,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("High-frequency spice enhancement"));
    parameters.addParameter(param);

    //---Pickle Ferment (Delay/Feedback)------------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleFerment,
        STR16("Pickle Ferment"),
        STR16("Ferment"),
        3,
        0,
        1.0,
        0.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Fermentation delay and feedback"));
    parameters.addParameter(param);

    //---Pickle Juice (Output Gain)-----------------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleJuice,
        STR16("Pickle Juice"),
        STR16("Juice"),
        4,
        0,
        2.0,
        1.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Output juice level"));
    parameters.addParameter(param);

    //---Pickle Processor (Multiband Compression)---------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleProcessor,
        STR16("Pickle Processor"),
        STR16("Processor"),
        5,
        0,
        1.0,
        0.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("OTT-style multiband compression for forwardness"));
    parameters.addParameter(param);

    //---Pickle Crunch New (Enhanced Crunch)--------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleCrunchNew,
        STR16("Pickle Crunch"),
        STR16("Crunch"),
        6,
        0,
        1.0,
        0.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Enhanced crunch distortion with low/high focus"));
    parameters.addParameter(param);

    //---Pickle Bitcrush (Sub-parameter)-------------------------------------
    param = new Steinberg::Vst::Parameter(
        Steinberg::Vst::ParameterInfo::kCanAutomate,
        PickleProcessor::kPickleBitcrush,
        STR16("Pickle Bitcrush"),
        STR16("Bitcrush"),
        7,
        0,
        1.0,
        0.0,
        Steinberg::Vst::ParameterInfo::kIsList,
        Steinberg::Vst::kRootUnitId,
        STR16("Subtle bitcrush effect below crunch"));
    parameters.addParameter(param);

    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleController::setComponentState(Steinberg::IBStream* state)
{
    if (!state)
        return Steinberg::kResultFalse;

    // Read component state and set parameters
    float brine = 0.5f, crunch = 0.0f, spice = 0.5f, ferment = 0.0f, juice = 1.0f;
    float processor = 0.0f, crunchNew = 0.0f, bitcrush = 0.0f;
    
    if (state->read(&brine, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&crunch, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&spice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&ferment, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&juice, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&processor, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&crunchNew, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;
    if (state->read(&bitcrush, sizeof(float)) != Steinberg::kResultTrue)
        return Steinberg::kResultFalse;

    // Set normalized parameter values
    setParamNormalized(PickleProcessor::kPickleBrine, brine);
    setParamNormalized(PickleProcessor::kPickleCrunch, crunch);
    setParamNormalized(PickleProcessor::kPickleSpice, spice);
    setParamNormalized(PickleProcessor::kPickleFerment, ferment);
    setParamNormalized(PickleProcessor::kPickleJuice, juice);
    setParamNormalized(PickleProcessor::kPickleProcessor, processor);
    setParamNormalized(PickleProcessor::kPickleCrunchNew, crunchNew);
    setParamNormalized(PickleProcessor::kPickleBitcrush, bitcrush);

    return Steinberg::kResultOk;
}

Steinberg::IPlugView* PLUGIN_API PickleController::createView(Steinberg::FIDString name)
{
    // Create and return our custom blurred glass UI view
    if (name && Steinberg::FIDStrings::strcmp(name, Steinberg::Vst::ViewType::kEditor) == 0)
    {
        return new PickleView(this);
    }
    return nullptr;
}

Steinberg::FUnknown* PickleController::createInstance(void* context)
{
    return static_cast<Steinberg::Vst::IEditController*>(new PickleController());
}

} // namespace Pickle
