import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction
import javax.inject.Inject
import org.gradle.process.ExecOperations

abstract class BuildTask : DefaultTask() {
    @get:Inject
    abstract val execOperations: ExecOperations

    @Input
    var rootDirRel: String? = null
    @Input
    var projectDir: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        val appDir = File(File(projectDir, rootDirRel), "..").canonicalFile
        execOperations.exec {
            workingDir(appDir)
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                executable("cmd")
                args("/c", "pnpm", "exec", "tauri", "android", "android-studio-script")
            } else {
                executable("pnpm")
                args("exec", "tauri", "android", "android-studio-script")
            }
            if (logger.isEnabled(LogLevel.DEBUG)) args("-vv")
            else if (logger.isEnabled(LogLevel.INFO)) args("-v")
            if (release) args("--release")
            args("--target", target)
        }.assertNormalExitValue()
    }
}
