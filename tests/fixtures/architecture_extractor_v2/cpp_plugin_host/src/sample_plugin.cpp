#include "plugin_api.hpp"

class SamplePlugin : public PluginApi {
public:
    const char* name() const override {
        return "sample_plugin";
    }
};
