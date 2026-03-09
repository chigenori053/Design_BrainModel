#include "plugin_api.hpp"

int start_plugin(PluginApi* plugin) {
    return plugin == 0 ? 1 : (plugin->name() == 0 ? 2 : 0);
}
