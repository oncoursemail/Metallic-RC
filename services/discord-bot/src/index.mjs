/**
 * Metallic Discord bot foundation: real-status API integration and safe public commands.
 * No public game backend exists yet; unavailable means unavailable, not zero players.
 * Production moderation commands are deliberately excluded until central RBAC API exists.
 */
import {
  Client, Events, GatewayIntentBits, ActivityType, MessageFlags,
} from "discord.js";
import { activityLabel, loadProductionStatus, publicReport } from "./status.mjs";

const token = process.env.METALLIC_DISCORD_TOKEN;
const appId = process.env.METALLIC_DISCORD_APP_ID;
const guildId = process.env.METALLIC_DISCORD_GUILD_ID;
const statusUrl = process.env.METALLIC_STATUS_URL || "";
const statusBearer = process.env.METALLIC_STATUS_BEARER || "";
const roadmapUrl = process.env.METALLIC_ROADMAP_URL ||
  "https://github.com/oncoursemail/Metallic-RC/blob/main/docs/ROADMAP.md";

if (!token || !appId || !guildId) {
  throw new Error("Bot requires METALLIC_DISCORD_TOKEN, _APP_ID and _GUILD_ID");
}

const client = new Client({ intents: [GatewayIntentBits.Guilds] });
let lastSnapshot = null;
let lastCheckedAt = 0;
let lastActivity = "";

async function snapshot() {
  const now = Date.now();
  if (now - lastCheckedAt < 60000) return lastSnapshot;
  lastCheckedAt = now;
  try {
    lastSnapshot = await loadProductionStatus(statusUrl, statusBearer, now);
  } catch (error) {
    // Never print tokens, full URLs, user data or status response bodies.
    console.warn("Trusted Metallic status endpoint unavailable:", error.name);
    lastSnapshot = null;
  }
  return lastSnapshot;
}

async function refreshActivity() {
  const current = await snapshot();
  const activity = activityLabel(current);
  if (client.user && lastActivity !== activity) {
    client.user.setPresence({
      activities: [{ name: activity, type: ActivityType.Playing }],
      status: "online",
    });
    lastActivity = activity;
  }
}

client.once(Events.ClientReady, async (ready) => {
  console.info("Metallic Discord integration connected as " + ready.user.id);
  await refreshActivity();
  setInterval(() => {
    void refreshActivity().catch(() => {});
  }, 120000).unref();
});

client.on(Events.InteractionCreate, async (interaction) => {
  if (!interaction.isChatInputCommand()) return;
  if (interaction.guildId !== guildId) {
    await interaction.reply({ content: "This command is available in the configured Metallic community server.", flags: MessageFlags.Ephemeral });
    return;
  }
  try {
    if (interaction.commandName === "help") {
      await interaction.reply({
        content: "**Metallic**\n" +
          "/players — real production player total if a verified feed is available\n" +
          "/status — game service availability\n" +
          "/roadmap — engineering plan and tracked progress\n" +
          "Moderation and owner commands will require Metallic's authenticated staff backend.",
        flags: MessageFlags.Ephemeral,
      });
    } else if (interaction.commandName === "players") {
      await interaction.deferReply();
      await interaction.editReply({ content: publicReport(await snapshot()) });
    } else if (interaction.commandName === "status") {
      await interaction.deferReply();
      const current = await snapshot();
      await interaction.editReply({
        content: current
          ? "**Metallic production status sample available.**\n" + publicReport(current)
          : "**Production status unavailable.** No verified real-player telemetry API is connected. This does not mean the service is online or offline.",
      });
    } else if (interaction.commandName === "roadmap") {
      await interaction.reply({ content: "Engineering roadmap: " + roadmapUrl +
        "\nNote: GitHub may require collaborator access while the repository is private.", flags: MessageFlags.Ephemeral });
    }
  } catch (error) {
    console.warn("Bot command failed:", error.name);
    const content = "Metallic could not complete that command. Please retry later.";
    if (interaction.deferred || interaction.replied) await interaction.editReply({ content });
    else await interaction.reply({ content, flags: MessageFlags.Ephemeral });
  }
});

await client.login(token);
