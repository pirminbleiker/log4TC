﻿using Mbc.Log4Tc.Dispatcher;
using Mbc.Log4Tc.Receiver;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Serilog;
using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace Mbc.Log4Tc.Service
{
    public static class Program
    {
        private static string[] CmdArgs;

        public static async Task Main(string[] args)
        {
            await CreateHostBuilder(args)
                .Build()
                .RunAsync();
        }

        public static IHostBuilder CreateHostBuilder(string[] args)
        {
            CmdArgs = args;
            var hostBuilder = Host.CreateDefaultBuilder(args)
                .ConfigureAppConfiguration(configure =>
                {
                    if (!IsLocalConfig())
                    {
                        // The place to find the appsettings.json
                        configure.SetBasePath(GetAppsettingsBasePath());
                    }
                })
                .ConfigureLogging(loggingBuilder =>
                {
                    loggingBuilder.ClearProviders();

                    var logPath = Path.Combine(GetInternalBasePath(), "service.log");
                    var logger = new LoggerConfiguration()
                        .Enrich.FromLogContext()
                        .WriteTo.Console()
                        .WriteTo.RollingFile(logPath, outputTemplate: "{Timestamp:yyyy-MM-dd HH:mm:ss.fff zzz} [{Level}] ({SourceContext}) {Message}{NewLine}{Exception}", fileSizeLimitBytes: 1024 * 1024 * 10, retainedFileCountLimit: 5)
                        .CreateLogger();

                    loggingBuilder.AddSerilog(logger: logger, dispose: true);
                })
                .ConfigureServices((hostContext, services) =>
                {
                    services
                        .AddPlugins(GetPluginPath())
                        // ToDo: Differenziate output / input / ... configuration in PluginBuilder
                        .AddOutputs(hostContext.Configuration);

                    services
                        .AddLog4TcAdsLogReceiver()
                        .AddLog4TcDispatcher();
                });

            if (args.Contains("--service", StringComparer.InvariantCultureIgnoreCase))
            {
                if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                {
                    hostBuilder.UseWindowsService();
                }
                else
                {
                    throw new PlatformNotSupportedException("Service only in windows system supported.");
                    /* For systemd install first package Microsoft.Extensions.Hosting.Systemd and switch to >= netcoreapp3.0
                     * https://devblogs.microsoft.com/dotnet/net-core-and-systemd/
                    if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux){
                        hostBuilder.UseSystemd();
                    }
                    */
                }
            }

            return hostBuilder;
        }

        private static string GetAppsettingsBasePath()
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                return Path.Combine(Environment.ExpandEnvironmentVariables("%programdata%"), "log4TC", "config");
            }
            else
            {
                throw new PlatformNotSupportedException("Service still in windows system supported.");
            }
        }

        private static string GetInternalBasePath()
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                return Path.Combine(Environment.ExpandEnvironmentVariables("%programdata%"), "log4TC", "internal");
            }
            else
            {
                throw new PlatformNotSupportedException("Service still in windows system supported.");
            }
        }

        private static string GetPluginPath()
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                if (IsLocalConfig())
                {
                    return @"../../plugins";
                }
                else
                {
                    return "plugins";
                }
            }
            else
            {
                throw new PlatformNotSupportedException("Service still in windows system supported.");
            }
        }

        private static bool IsLocalConfig()
        {
            return CmdArgs.Contains("--localconfig", StringComparer.InvariantCultureIgnoreCase);
        }
    }
}
