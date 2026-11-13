using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using System.Security.Cryptography;
using System.Text.Json;

if(args.Length<2)throw new ArgumentException("SyntaxCheck NEW-report.json source-directory [source-directory ...]");
string report=Path.GetFullPath(args[0]);if(File.Exists(report))throw new IOException("Report already exists");
var options=new CSharpParseOptions(LanguageVersion.Preview,DocumentationMode.None,SourceCodeKind.Regular);
var paths=args.Skip(1).SelectMany(p=>Directory.GetFiles(p,"*.partial.cs")).OrderBy(p=>p,StringComparer.Ordinal).ToArray();
if(paths.Length==0)throw new InvalidDataException("No generated C# files found");
var rows=new List<object>();long totalErrors=0;int parsed=0;
foreach(var path in paths){
    byte[] bytes=File.ReadAllBytes(path);string source=System.Text.Encoding.UTF8.GetString(bytes);
    var tree=CSharpSyntaxTree.ParseText(source,options,path);
    var diagnostics=tree.GetDiagnostics().Where(d=>d.Severity==DiagnosticSeverity.Error).ToArray();
    totalErrors+=diagnostics.Length;parsed++;
    rows.Add(new{file=Path.GetFileName(path),bytes=bytes.Length,sha256=Convert.ToHexStringLower(SHA256.HashData(bytes)),syntaxErrors=diagnostics.Length,firstErrors=diagnostics.Take(12).Select(d=>new{id=d.Id,message=d.GetMessage(),line=d.Location.GetLineSpan().StartLinePosition.Line+1}).ToArray()});
    if(parsed%50==0)Console.WriteLine(JsonSerializer.Serialize(new{parsed,total=paths.Length,totalErrors}));
}
var result=new{status=totalErrors==0?"syntax_parse_passed":"syntax_errors_recorded",generatedFiles=paths.Length,syntaxErrors=totalErrors,parserVersion=typeof(CSharpSyntaxTree).Assembly.GetName().Version?.ToString(),languageVersion=options.LanguageVersion.ToString(),semanticCompilationPerformed=false,targetCodeExecuted=false,limit="Syntax acceptance is not type checking, a successful Unity build, or gameplay equivalence.",files=rows};
File.WriteAllText(report,JsonSerializer.Serialize(result,new JsonSerializerOptions{WriteIndented=true})+"\n");
Console.WriteLine(JsonSerializer.Serialize(new{result.status,result.generatedFiles,result.syntaxErrors,result.parserVersion}));
return totalErrors==0?0:1;
