'use strict';
// Only used inside a caller-owned, explicitly selected test client process.
// No machine DNS/certificate/hosts changes. No authorization or TLS checks bypassed.
const MetallicRoutePolicy = (() => {
  const services = new Set(['ai','auth','api','www','notifications','images','cdn','commerce','matchmaking','storage','chat','leaderboard','accounts','link','lists','roomcomments','clubs','rooms','platformnotifications','moderation','datacollection','bugreporting','discovery','playersettings','studio','gamelogs','strings','econ','stringscdn','data','cards']);
  function origin(port) {
    if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error('Invalid explicit loopback port');
    return 'http://127.0.0.1:' + port;
  }
  function route(value, port) {
    const local = origin(port);
    if (typeof value !== 'string' || value.length > 8192) return {action:'reject',reason:'input_bound'};
    if (/^[\u0000-\u0020]|[\r\n\u0000\\]/.test(value)) return {action:'reject',reason:'ambiguous_uri'};
    const match = /^(https?|wss?):\/\/([^/?#]+)([^#]*)(?:#.*)?$/i.exec(value);
    if (!match) return {action:'unchanged',reason:'not_absolute_network_uri'};
    const scheme = match[1].toLowerCase(), authority = match[2].toLowerCase();
    const rest = match[3] || '/';
    if (authority.includes('@')) return {action:'redirect',reason:'userinfo_blocked',url:local+'/blocked-external/'};
    if (authority === '127.0.0.1:'+port && scheme === 'http') return {action:'unchanged',reason:'owned_origin'};
    const host = authority.replace(/:\d+$/, '');
    if (host === 'ns.rec.net') return {action:'redirect',reason:'nameserver',url:local+(rest.startsWith('/')?rest:'/'+rest)};
    const serviceMatch = /^([a-z]+)\.rec\.net$/.exec(host);
    if (serviceMatch && services.has(serviceMatch[1]))
      return {action:'redirect',reason:'service',service:serviceMatch[1],url:local+'/recnet/'+serviceMatch[1]+(rest.startsWith('/')?rest:'/'+rest)};
    return {action:'redirect',reason:'unmapped_external_blocked',url:local+'/blocked-external/'};
  }
  function diagnosticPath(value) {
    if (typeof value !== 'string') return 'unreadable';
    const match=/^[a-z]+:\/\/[^/?#]+([^?#]*)/.exec(value);
    const path=(match ? match[1] : '').slice(0,512);
    return (path||'/').split('/').map(segment => {
      if (segment.length > 48 || /@|%|=/.test(segment) || /^[0-9a-f]{16,}$/i.test(segment) || /^\d+$/.test(segment)) return ':redacted';
      return /^[a-zA-Z0-9_.-]*$/.test(segment) ? segment : ':redacted';
    }).join('/');
  }
  return {route,diagnosticPath,origin};
})();
if (typeof module !== 'undefined') module.exports = MetallicRoutePolicy;
