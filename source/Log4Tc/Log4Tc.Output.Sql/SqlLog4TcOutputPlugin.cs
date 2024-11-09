using Log4Tc.Plugin;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;

namespace Log4Tc.Output.Sql
{
    public class SqlLog4TcOutputPlugin : IPlugin
    {
        public void ConfigureServices(IServiceCollection services, IConfiguration configuration)
        {
            services.AddSingleton<IOutputFactory, SqlLog4TcOutputFactory>();
        }
    }
}
