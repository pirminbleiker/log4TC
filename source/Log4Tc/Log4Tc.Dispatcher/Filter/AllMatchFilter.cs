﻿using Log4Tc.Model;

namespace Log4Tc.Dispatcher.Filter
{
    /// <summary>
    /// A <see cref="ILogFilter"/> which matches any <see cref="LogEntry"/>.
    /// </summary>
    public class AllMatchFilter : ILogFilter
    {
        public static readonly AllMatchFilter Default = new AllMatchFilter();

        public bool Matches(LogEntry logEntry) => true;

        public override string ToString() => "Filter(*)";
    }
}
