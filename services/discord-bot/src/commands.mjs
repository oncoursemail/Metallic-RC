import { SlashCommandBuilder } from "discord.js";

export const commands = [
  new SlashCommandBuilder().setName("help").setDescription("See Metallic community commands"),
  new SlashCommandBuilder().setName("players").setDescription("Show server-reported human players online"),
  new SlashCommandBuilder().setName("status").setDescription("Show Metallic production service status"),
  new SlashCommandBuilder().setName("roadmap").setDescription("Open the engineering roadmap"),
].map(command => command.toJSON());
