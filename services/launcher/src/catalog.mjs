const freeze = value => {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
};
export const PLATFORMS = freeze(['windows-x64','windows-arm64','macos-x64','macos-arm64','linux-x64','linux-arm64']);
export const CATALOG = freeze([
  { id:'metallic-game', kind:'component', title:'Metallic', mark:'M',
    description:'Social worlds, avatars, and community in the original Metallic game.',
    detail:'The game only. Hosting is a separate choice.', requires:[], distribution:null,
    unavailableReason:'A signed Metallic game release has not been configured.' },
  { id:'minecraft-java', kind:'component', title:'Minecraft Java', mark:'MC',
    description:'Add Minecraft Java to your launcher using your own licensed account.',
    detail:'Includes only the Java runtime required by the selected Minecraft version.',
    requires:['minecraft-runtime'], distribution:null,
    unavailableReason:'Microsoft sign-in and the official Minecraft installer are not configured.' },
  { id:'metallic-host', kind:'component', title:'Metallic Host', mark:'H',
    description:'Host a Minecraft server on your own hardware through Metallic.',
    detail:'Optional. Playing either game does not require hosting.', requires:[], distribution:null,
    unavailableReason:'A signed host-agent package and device enrollment are not configured.' },
  { id:'minecraft-runtime', kind:'runtime', title:'Minecraft Java runtime',
    requires:[], distribution:null,
    unavailableReason:'The Java runtime must be resolved from the selected official Minecraft version.' },
]);
