using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;

namespace Log4Tc.Plugin
{
    public interface IPlugin
    {
        void ConfigureServices(IServiceCollection services, IConfiguration configuration);
    }
}
