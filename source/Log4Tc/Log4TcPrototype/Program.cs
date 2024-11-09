using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using TwinCAT.Ads;
using TwinCAT.Ads.Server;

namespace Log4TcPrototype
{
    public static class Program
    {
        public static void Main()
        {
            var server = new LogServer();
            server.ConnectServer();

            Console.WriteLine("Server is ready.");
            Console.ReadKey();

            server.Disconnect();
        }
    }

#pragma warning disable SA1402 // File may only contain a single type
    internal class LogServer : AdsServer
#pragma warning restore SA1402 // File may only contain a single type
    {
        public LogServer()
            : base(16150, "Log4TC")
        {
        }

        protected override Task<ResultWrite> OnWriteAsync(AmsAddress target, uint invokeId, uint indexGroup, uint indexOffset, ReadOnlyMemory<byte> writeData, CancellationToken cancel)
        {
            Console.WriteLine($"Request from {target}");

            try
            {
                using var stream = new MemoryStream(writeData.ToArray());
                using var reader = new BinaryReader(stream);

                while (reader.BaseStream.Length > reader.BaseStream.Position)
                {
                    var version = reader.ReadByte();
                    byte[] buf = new byte[256];
                    byte ch;
                    int i = 0;
                    while ((ch = reader.ReadByte()) != 0)
                    {
                        buf[i++] = ch;
                    }

                    var message = Encoding.Default.GetString(buf, 0, i);

                    i = 0;
                    while ((ch = reader.ReadByte()) != 0)
                    {
                        buf[i++] = ch;
                    }

                    var logger = Encoding.Default.GetString(buf, 0, i);

                    var level = reader.ReadUInt16();
                    var timestampPlc = DateTime.FromFileTime(reader.ReadInt64());
                    var timestampClock = DateTime.FromFileTime(reader.ReadInt64());

                    var args = new List<(int, object)>();
                    var contex = new List<(string, object)>();
                    byte type;
                    while ((type = reader.ReadByte()) != 255)
                    {
                        if (type == 1)
                        {
                            // Argument
                            var argNo = reader.ReadByte();
                            var argType = reader.ReadInt16();
                            object argValue = null;
                            switch (argType)
                            {
                                case 4: // REAL
                                    argValue = reader.ReadSingle();
                                    break;

                                case 7: // INT
                                    argValue = reader.ReadInt16();
                                    break;

                                case 12: // STRING
                                    i = 0;
                                    while ((ch = reader.ReadByte()) != 0)
                                    {
                                        buf[i++] = ch;
                                    }

                                    argValue = Encoding.Default.GetString(buf, 0, i);
                                    break;
                            }

                            args.Add((argNo, argValue));
                        }
                        else if (type == 2)
                        {
                            i = 0;
                            while ((ch = reader.ReadByte()) != 0)
                            {
                                buf[i++] = ch;
                            }

                            var name = Encoding.Default.GetString(buf, 0, i);
                            var valueType = reader.ReadInt16();
                            var value = reader.ReadInt16();
                            contex.Add((name, value));
                        }
                    }

                    Console.WriteLine($"Log-Entry: version={version} message={message} logger={logger} level={level} timestamp={timestampPlc}.{timestampPlc.Millisecond} args=[{string.Join(",", args)}] context=[{string.Join(",", contex)}]");
                }

                return Task.FromResult(new ResultWrite(AdsErrorCode.NoError));
            }
            catch (Exception e)
            {
                Console.WriteLine($"Error {e}");
                return Task.FromResult(new ResultWrite(AdsErrorCode.DeviceError));
            }
        }
    }
}
