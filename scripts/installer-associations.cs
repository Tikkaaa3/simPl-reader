// Read-only Shell API probe for installer integration tests; no default changes.
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class InstallerAssociations
{
    [ComImport, Guid("F04061AC-1659-4A3F-A954-775AA57FC083"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IAssocHandler
    {
        void GetName([MarshalAs(UnmanagedType.LPWStr)] out string name);
        void GetUIName([MarshalAs(UnmanagedType.LPWStr)] out string name);
    }

    [ComImport, Guid("973810AE-9599-4B88-9E4D-6EE98C9552DA"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IEnumAssocHandlers
    {
        [PreserveSig] int Next(uint count, out IAssocHandler handler, out uint fetched);
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int SHAssocEnumHandlers(string extension, uint filter, out IEnumAssocHandlers handlers);

    public static string[] Recommended(string extension)
    {
        IEnumAssocHandlers handlers;
        Marshal.ThrowExceptionForHR(SHAssocEnumHandlers(extension, 1, out handlers));
        var result = new List<string>();
        try
        {
            IAssocHandler handler;
            uint fetched;
            int status;
            while ((status = handlers.Next(1, out handler, out fetched)) == 0 && fetched == 1)
            {
                try
                {
                    string path, name;
                    handler.GetName(out path);
                    handler.GetUIName(out name);
                    result.Add(name + "|" + path);
                }
                finally { Marshal.ReleaseComObject(handler); }
            }
            Marshal.ThrowExceptionForHR(status);
        }
        finally { Marshal.ReleaseComObject(handlers); }
        return result.ToArray();
    }
}
