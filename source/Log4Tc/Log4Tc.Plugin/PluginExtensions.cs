using Log4Tc.Output;
using Log4Tc.Plugin;
using Microsoft.Extensions.DependencyInjection;
using System;
using System.Collections.Generic;
using System.Linq;

namespace Log4Tc.Service
{
    public static class PluginExtensions
    {
        public static PluginBuilder AddPlugins(this IServiceCollection services, string pluginFolder)
        {
            // Check Valid Parameters
            if (!Uri.IsWellFormedUriString(pluginFolder, UriKind.RelativeOrAbsolute))
            {
                throw new ApplicationException($"The plugin folder {pluginFolder} is not well formed.");
            }

            // Load the Plugin Assemblies and Create Plugin instance
            IEnumerable<IPlugin> plugins = PluginLoader.LoadPlugins(pluginFolder).ToList();

            return new PluginBuilder(services, plugins);
        }
    }
}
