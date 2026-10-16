import { REST, Routes } from "discord.js";
import { commands } from "./commands.mjs";

const token = process.env.METALLIC_DISCORD_TOKEN;
const appId = process.env.METALLIC_DISCORD_APP_ID;
const guildId = process.env.METALLIC_DISCORD_GUILD_ID;

if (!token || !appId || !guildId) {
  throw new Error("Register requires METALLIC_DISCORD_TOKEN, _APP_ID and _GUILD_ID");
}

// Guild-scoped registration is deliberate: do not overwrite global commands.
const rest = new REST({ version: "10" }).setToken(token);
const response = await rest.put(Routes.applicationGuildCommands(appId, guildId), { body: commands });
console.log("Registered " + response.length + " Metallic slash commands for configured development guild");
