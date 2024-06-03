import com.google.protobuf.gradle.id

plugins {
    id("java")
    id("io.micronaut.application") version "4.4.0"
    id("io.micronaut.test-resources") version "4.4.0"
    id("com.google.protobuf") version "0.9.4"
}

group = "tech.code0"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

dependencies {

    annotationProcessor("io.micronaut.serde:micronaut-serde-processor")
    implementation("io.micronaut:micronaut-discovery-core")
    implementation("io.micronaut.grpc:micronaut-grpc-runtime")
    implementation("io.micronaut.rabbitmq:micronaut-rabbitmq")
    implementation("io.micronaut.redis:micronaut-redis-lettuce")
    implementation("io.micronaut.serde:micronaut-serde-jackson")
    implementation("javax.annotation:javax.annotation-api")
    runtimeOnly("ch.qos.logback:logback-classic")

    implementation("com.gitlab.taucher2003.t2003-utils:log:1.1-beta.13")
    compileOnly("org.projectlombok:lombok:1.18.32")
    annotationProcessor("org.projectlombok:lombok:1.18.32")

}

application {
    mainClass = "tech.code0.AquilaApplication"
}
java {
    sourceCompatibility = JavaVersion.toVersion("22")
    targetCompatibility = JavaVersion.toVersion("22")
}

sourceSets {
    main {
        java {
            srcDirs("build/generated/source/proto/main/grpc")
            srcDirs("build/generated/source/proto/main/java")
        }
    }
}

micronaut {
    testRuntime("junit5")
    processing {
        incremental(true)
        annotations("tech.code0.*")
    }
    testResources {
        sharedServer = true
    }
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.27.0"
    }

    plugins {
        id("grpc") {
            artifact = "io.grpc:protoc-gen-grpc-java:1.64.0"
        }
    }

    generateProtoTasks {
        all().forEach { task ->
            task.plugins {
                id("grpc")
            }
        }
    }
}

tasks.withType<JavaCompile> {
    options.compilerArgs.add("--enable-preview")
}

tasks.named<io.micronaut.gradle.docker.NativeImageDockerfile>("dockerfileNative") {
    jdkVersion = "22"
}