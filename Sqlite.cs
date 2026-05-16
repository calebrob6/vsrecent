// Minimal P/Invoke wrapper around Windows' built-in winsqlite3.dll.
// Used to read a single TEXT cell from VSCode's read-only state DB.
using System;
using System.Runtime.InteropServices;
using System.Text;

namespace VsRecent
{
    internal static class Sqlite
    {
        private const string Lib = "winsqlite3";

        public const int SQLITE_OK   = 0;
        public const int SQLITE_ROW  = 100;
        public const int SQLITE_DONE = 101;

        public const int SQLITE_OPEN_READONLY = 0x00000001;
        public const int SQLITE_OPEN_NOMUTEX  = 0x00008000;

        [DllImport(Lib, EntryPoint = "sqlite3_open_v2", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_open_v2(byte[] filename, out IntPtr db, int flags, IntPtr zVfs);

        [DllImport(Lib, EntryPoint = "sqlite3_close", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_close(IntPtr db);

        [DllImport(Lib, EntryPoint = "sqlite3_prepare_v2", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_prepare_v2(IntPtr db, byte[] zSql, int nByte,
                                                     out IntPtr stmt, IntPtr pzTail);

        [DllImport(Lib, EntryPoint = "sqlite3_step", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_step(IntPtr stmt);

        [DllImport(Lib, EntryPoint = "sqlite3_column_text", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sqlite3_column_text(IntPtr stmt, int iCol);

        [DllImport(Lib, EntryPoint = "sqlite3_column_bytes", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_column_bytes(IntPtr stmt, int iCol);

        [DllImport(Lib, EntryPoint = "sqlite3_finalize", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_finalize(IntPtr stmt);

        [DllImport(Lib, EntryPoint = "sqlite3_errmsg", ExactSpelling = true,
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sqlite3_errmsg(IntPtr db);

        // Open dbPath read-only and return the value of the single column from
        // the first row of the given SQL. Returns null if no row.
        public static string ReadSingleText(string dbPath, string sql)
        {
            IntPtr db = IntPtr.Zero;
            IntPtr stmt = IntPtr.Zero;
            try
            {
                byte[] fn = NullTermUtf8(dbPath);
                int rc = sqlite3_open_v2(fn, out db,
                    SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, IntPtr.Zero);
                if (rc != SQLITE_OK)
                    throw new InvalidOperationException("sqlite3_open_v2 failed (rc=" + rc + ") for " + dbPath);

                byte[] sqlBytes = NullTermUtf8(sql);
                rc = sqlite3_prepare_v2(db, sqlBytes, sqlBytes.Length, out stmt, IntPtr.Zero);
                if (rc != SQLITE_OK)
                    throw new InvalidOperationException("sqlite3_prepare_v2 failed (rc=" + rc + "): " + ReadErr(db));

                rc = sqlite3_step(stmt);
                if (rc == SQLITE_ROW)
                {
                    IntPtr p = sqlite3_column_text(stmt, 0);
                    int n = sqlite3_column_bytes(stmt, 0);
                    if (p == IntPtr.Zero || n <= 0) return string.Empty;
                    byte[] buf = new byte[n];
                    Marshal.Copy(p, buf, 0, n);
                    return Encoding.UTF8.GetString(buf);
                }
                return null;
            }
            finally
            {
                if (stmt != IntPtr.Zero) sqlite3_finalize(stmt);
                if (db != IntPtr.Zero) sqlite3_close(db);
            }
        }

        private static byte[] NullTermUtf8(string s)
        {
            int n = Encoding.UTF8.GetByteCount(s);
            byte[] bytes = new byte[n + 1];
            Encoding.UTF8.GetBytes(s, 0, s.Length, bytes, 0);
            return bytes;
        }

        private static string ReadErr(IntPtr db)
        {
            IntPtr p = sqlite3_errmsg(db);
            if (p == IntPtr.Zero) return "(null)";
            int n = 0;
            while (Marshal.ReadByte(p, n) != 0) n++;
            byte[] buf = new byte[n];
            Marshal.Copy(p, buf, 0, n);
            return Encoding.UTF8.GetString(buf);
        }
    }
}
