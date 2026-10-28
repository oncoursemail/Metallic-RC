using System.Net.WebSockets;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Channels;

namespace Metallic.RC.Backend;

// Metallic protocol v1 presence/pose replication, not recovered Rec Room gameplay transport.
public sealed class RoomHub(BackendStore store, IHostApplicationLifetime lifetime)
{
    private readonly object gate = new();
    private readonly Dictionary<string, LiveRoom> rooms = new(StringComparer.Ordinal);
    private int peerCount;
    public int Connections { get { lock(gate)return peerCount; } }
    private sealed class LiveRoom
    {
        public string Epoch { get; } = Guid.NewGuid().ToString("N");
        public long Revision;
        public Dictionary<string, Peer> Peers { get; } = new(StringComparer.Ordinal);
    }
    private sealed class Peer(Principal principal, CancellationToken request, CancellationToken stop) : IDisposable
    {
        public Principal Principal { get; } = principal;
        public WebSocket? Socket;
        public bool Ready;
        public Channel<byte[]> Outgoing { get; } = Channel.CreateBounded<byte[]>(new BoundedChannelOptions(64){SingleReader=true,FullMode=BoundedChannelFullMode.Wait});
        public CancellationTokenSource Cancellation { get; } = CancellationTokenSource.CreateLinkedTokenSource(request,stop);
        public long Sequence;
        public double X,Y,Z;
        public void Queue(object message)
        {
            if(!Outgoing.Writer.TryWrite(JsonSerializer.SerializeToUtf8Bytes(message, BackendJson.Options)))
            {Cancellation.Cancel();Socket?.Abort();}
        }
        public void Dispose(){Cancellation.Cancel();Outgoing.Writer.TryComplete();Socket?.Abort();Socket?.Dispose();Cancellation.Dispose();}
    }
    private sealed record PoseMessage(
        [property: JsonRequired] string Type,
        [property: JsonRequired] long Sequence,
        [property: JsonRequired] double X,
        [property: JsonRequired] double Y,
        [property: JsonRequired] double Z);
    public void Revoke(string tokenHash)
    {
        lock(gate)
            foreach(var peer in rooms.Values.SelectMany(r=>r.Peers.Values).Where(p=>p.Principal.TokenHash==tokenHash))
            {peer.Cancellation.Cancel();peer.Socket?.Abort();}
    }
    public async Task Connect(HttpContext context, Principal principal, RoomView room)
    {
        if(!context.WebSockets.IsWebSocketRequest)throw new BackendFault(400,"websocket_required");
        var peer=new Peer(principal,context.RequestAborted,lifetime.ApplicationStopping);
        LiveRoom live;
        lock(gate)
        {
            if(peerCount>=256){peer.Dispose();throw new BackendFault(503,"connection_limit");}
            if(!rooms.TryGetValue(room.Id,out live!))
            {
                if(rooms.Count>=128){peer.Dispose();throw new BackendFault(503,"active_room_limit");}
                live=new LiveRoom();rooms.Add(room.Id,live);
            }
            if(live.Peers.Count>=room.Capacity||live.Peers.ContainsKey(principal.AccountId))
            {peer.Dispose();throw new BackendFault(409,"room_full_or_already_connected");}
            live.Peers.Add(principal.AccountId,peer);peerCount++;
        }
        Task sending=Task.CompletedTask,monitoring=Task.CompletedTask;
        try
        {
            peer.Socket=await context.WebSockets.AcceptWebSocketAsync();
            lock(gate)
            {
                peer.Ready=true;
                peer.Queue(new{type="snapshot",protocol="metallic-room-v1",roomId=room.Id,epoch=live.Epoch,revision=live.Revision,accountId=principal.AccountId,
                    peers=live.Peers.Values.Where(p=>p.Ready).Select(p=>new{accountId=p.Principal.AccountId,sequence=p.Sequence,x=p.X,y=p.Y,z=p.Z}).ToArray()});
                Broadcast(live,new{type="joined",epoch=live.Epoch,revision=++live.Revision,accountId=principal.AccountId});
            }
            sending=Send(peer);monitoring=Monitor(peer);
            byte[] buffer=new byte[4096];long window=Environment.TickCount64;int messages=0;
            while(!peer.Cancellation.IsCancellationRequested)
            {
                int count=0;WebSocketReceiveResult frame;
                using var idle=CancellationTokenSource.CreateLinkedTokenSource(peer.Cancellation.Token);idle.CancelAfter(TimeSpan.FromSeconds(60));
                do
                {
                    frame=await peer.Socket.ReceiveAsync(new ArraySegment<byte>(buffer,count,buffer.Length-count),idle.Token);
                    if(frame.MessageType==WebSocketMessageType.Close)return;
                    count+=frame.Count;
                    if(frame.MessageType!=WebSocketMessageType.Text||count>=buffer.Length)throw new BackendFault(400,"frame_rejected");
                }while(!frame.EndOfMessage);
                if(Environment.TickCount64-window>=1000){window=Environment.TickCount64;messages=0;}
                if(++messages>60)throw new BackendFault(429,"message_rate_exceeded");
                store.AuthenticateHash(principal.TokenHash);
                try
                {
                    BackendJson.RejectDuplicateProperties(buffer.AsSpan(0,count));
                    var message=JsonSerializer.Deserialize<PoseMessage>(buffer.AsSpan(0,count),BackendJson.Options)??throw new JsonException();
                    if(message.Type!="pose"||message.Sequence<=0||!Finite(message.X)||!Finite(message.Y)||!Finite(message.Z))throw new BackendFault(400,"invalid_pose");
                    lock(gate)
                    {
                        if(message.Sequence<=peer.Sequence)throw new BackendFault(409,"stale_sequence");
                        peer.Sequence=message.Sequence;peer.X=message.X;peer.Y=message.Y;peer.Z=message.Z;
                        Broadcast(live,new{type="pose",epoch=live.Epoch,revision=++live.Revision,accountId=principal.AccountId,sequence=peer.Sequence,x=peer.X,y=peer.Y,z=peer.Z});
                    }
                }
                catch(JsonException){peer.Queue(new{type="error",code="invalid_json"});}
                catch(BackendFault error){peer.Queue(new{type="error",code=error.Code});}
            }
        }
        catch(Exception ex)when(ex is WebSocketException or OperationCanceledException or BackendFault or IOException) { }
        finally
        {
            try
            {
                peer.Cancellation.Cancel();peer.Socket?.Abort();
                try{await Task.WhenAll(sending,monitoring);}catch(Exception ex)when(ex is WebSocketException or OperationCanceledException or BackendFault or IOException){}
            }
            finally
            {
                // An unexpected sender/monitor failure must not retain a room slot.
                try
                {
                    lock(gate)
                    {
                        live.Peers.Remove(principal.AccountId);peerCount--;
                        try{if(peer.Ready)Broadcast(live,new{type="left",epoch=live.Epoch,revision=++live.Revision,accountId=principal.AccountId});}
                        finally{if(live.Peers.Count==0)rooms.Remove(room.Id);}
                    }
                }
                finally{peer.Dispose();}
            }
        }
    }
    private static bool Finite(double value)=>double.IsFinite(value)&&Math.Abs(value)<=10000;
    private static void Broadcast(LiveRoom room,object message){foreach(var peer in room.Peers.Values)if(peer.Ready)peer.Queue(message);}
    private static async Task Send(Peer peer)
    {
        try
        {
            await foreach(var bytes in peer.Outgoing.Reader.ReadAllAsync(peer.Cancellation.Token))
                await peer.Socket!.SendAsync(bytes,WebSocketMessageType.Text,true,peer.Cancellation.Token);
        }
        finally{peer.Cancellation.Cancel();peer.Socket?.Abort();}
    }
    private async Task Monitor(Peer peer)
    {
        try
        {
            using var timer=new PeriodicTimer(TimeSpan.FromSeconds(1));
            while(await timer.WaitForNextTickAsync(peer.Cancellation.Token))store.AuthenticateHash(peer.Principal.TokenHash);
        }
        finally{peer.Cancellation.Cancel();peer.Socket?.Abort();}
    }
}
