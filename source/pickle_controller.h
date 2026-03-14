#pragma once

#include "public.sdk/source/vst/vsteditcontroller.h"
#include "pickle_processor.h"

namespace Pickle {

class PickleController : public Steinberg::Vst::EditController
{
public:
    PickleController() = default;
    ~PickleController() override = default;

    //--- EditController overrides -------------------------------------------
    Steinberg::tresult PLUGIN_API initialize(Steinberg::FUnknown* context) override;
    Steinberg::tresult PLUGIN_API setComponentState(Steinberg::IBStream* state) override;
    Steinberg::IPlugView* PLUGIN_API createView(Steinberg::FIDString name) override;

    //--- Static methods ------------------------------------------------------
    static Steinberg::FUnknown* createInstance(void* context);

    //--- Controller UID ------------------------------------------------------
    static const Steinberg::FUID uid;
};

} // namespace Pickle
