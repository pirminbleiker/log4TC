using InfluxDB.Client.Writes;
using Log4Tc.Model;

namespace Log4Tc.Output.InfluxDb
{
    internal interface IInfluxPointFactory
    {
        PointData CreatePoint(LogEntry logEntry);
    }
}
