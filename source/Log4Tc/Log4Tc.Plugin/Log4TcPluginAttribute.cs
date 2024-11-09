using System;

namespace Log4Tc.Plugin
{
    [AttributeUsage(AttributeTargets.Assembly)]
    public class Log4TcPluginAttribute : Attribute
    {
        public Log4TcPluginAttribute()
        {
        }
    }
}
