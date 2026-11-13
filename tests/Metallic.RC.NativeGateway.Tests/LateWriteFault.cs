using System.Runtime.InteropServices;

// TEST PROCESS ONLY. The pinned SQLitePCL wrapper exposes authorizer/update
// hooks but not auto_extension. No extension file, production seam or DB schema
// is added. Only this process's existing bundled SQLite module is used.
sealed class LateWriteFault : IDisposable
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int Entry(IntPtr db,IntPtr error,IntPtr api);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int Authorizer(IntPtr context,int operation,IntPtr first,IntPtr second,IntPtr database,IntPtr trigger);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate void Updated(IntPtr context,int operation,IntPtr database,IntPtr table,long rowid);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int Register(IntPtr callback);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate int SetAuthorizer(IntPtr db,IntPtr callback,IntPtr context);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] delegate IntPtr SetUpdate(IntPtr db,IntPtr callback,IntPtr context);
    readonly IntPtr library,entryPointer,authorizerPointer,updatePointer;
    readonly Entry entry;readonly Authorizer authorizer;readonly Updated updated;
    readonly Register unregister;readonly SetAuthorizer setAuthorizer;readonly SetUpdate setUpdate;
    readonly CancellationTokenSource? cancelAfterWrite;
    int sessionWrites,historyWrites,keyWrites,denied,callbackFailure;
    public int SessionWrites=>Volatile.Read(ref sessionWrites);
    public int HistoryWrites=>Volatile.Read(ref historyWrites);
    public int Denied=>Volatile.Read(ref denied);
    public int KeyWrites=>Volatile.Read(ref keyWrites);
    public bool CallbackFailed=>Volatile.Read(ref callbackFailure)!=0;
    public LateWriteFault(CancellationTokenSource? cancellation=null)
    {
        cancelAfterWrite=cancellation;
        string rid=(OperatingSystem.IsWindows()?"win-":"linux-")+RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant();
        string path=Path.Combine(AppContext.BaseDirectory,"runtimes",rid,"native",OperatingSystem.IsWindows()?"e_sqlite3.dll":"libe_sqlite3.so");
        if(!File.Exists(path))throw new IOException("Expected bundled SQLite test dependency is absent.");
        library=NativeLibrary.Load(path);
        T Symbol<T>(string name)where T:Delegate=>Marshal.GetDelegateForFunctionPointer<T>(NativeLibrary.GetExport(library,name));
        unregister=Symbol<Register>("sqlite3_cancel_auto_extension");setAuthorizer=Symbol<SetAuthorizer>("sqlite3_set_authorizer");setUpdate=Symbol<SetUpdate>("sqlite3_update_hook");
        authorizer=Authorize;updated=Update;entry=Initialize;
        authorizerPointer=Marshal.GetFunctionPointerForDelegate(authorizer);updatePointer=Marshal.GetFunctionPointerForDelegate(updated);entryPointer=Marshal.GetFunctionPointerForDelegate(entry);
        if(Symbol<Register>("sqlite3_auto_extension")(entryPointer)!=0){NativeLibrary.Free(library);throw new IOException("SQLite test callback registration failed.");}
    }
    int Initialize(IntPtr db,IntPtr error,IntPtr api)
    {
        try{if(setAuthorizer(db,authorizerPointer,IntPtr.Zero)!=0)return 1;setUpdate(db,updatePointer,IntPtr.Zero);return 0;}
        catch{Interlocked.Exchange(ref callbackFailure,1);return 1;}
    }
    int Authorize(IntPtr context,int operation,IntPtr first,IntPtr second,IntPtr database,IntPtr trigger)
    {
        try
        {
            if(cancelAfterWrite is null&&operation==18&&Marshal.PtrToStringUTF8(first)=="native_gateway_request_keys")
            {
                if(SessionWrites<1||HistoryWrites<2)Interlocked.Exchange(ref callbackFailure,1);
                Interlocked.Increment(ref denied);return 1; // SQLITE_DENY -> actual SQLITE_AUTH.
            }
            return 0;
        }
        catch{Interlocked.Exchange(ref callbackFailure,1);return 1;}
    }
    void Update(IntPtr context,int operation,IntPtr database,IntPtr table,long rowid)
    {
        try
        {
            if(operation!=18)return;
            string? name=Marshal.PtrToStringUTF8(table);
            if(name=="native_sessions"){Interlocked.Increment(ref sessionWrites);cancelAfterWrite?.Cancel();}
            if(name=="native_session_tokens")Interlocked.Increment(ref historyWrites);
            if(name=="native_gateway_request_keys")Interlocked.Increment(ref keyWrites);
        }
        catch{Interlocked.Exchange(ref callbackFailure,1);}
    }
    public void Dispose()
    {
        // Call only after the awaited request/commands have disposed their
        // Pooling=false connections. Do not clear somebody else's extensions.
        int result=unregister(entryPointer);
        GC.KeepAlive(entry);GC.KeepAlive(authorizer);GC.KeepAlive(updated);
        NativeLibrary.Free(library);
        if(result!=1)throw new IOException("Owned SQLite callback was not unregistered.");
    }
}
