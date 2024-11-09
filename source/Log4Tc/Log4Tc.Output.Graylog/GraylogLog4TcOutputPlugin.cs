using Log4Tc.Plugin;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;

namespace Log4Tc.Output.Graylog
{
    public class GraylogLog4TcOutputPlugin : IPlugin
    {
        public void ConfigureServices(IServiceCollection services, IConfiguration configuration)
        {
            services.AddSingleton<IOutputFactory, GraylogLog4TcOutputFactory>();
        }
    }
}
