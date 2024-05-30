import com.google.protobuf.gradle.id

plugins {
    id("java")
    id("com.google.protobuf") version "0.9.4"
}

group = "tech.code0"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

dependencies {
    implementation("com.rabbitmq:amqp-client:5.21.0")
    implementation("io.lettuce:lettuce-core:6.3.2.RELEASE")
    implementation("com.gitlab.taucher2003.t2003-utils:log:1.1-beta.13")
    compileOnly("org.projectlombok:lombok:1.18.32")
    annotationProcessor("org.projectlombok:lombok:1.18.32")

    implementation("io.grpc:grpc-netty-shaded:1.64.0")
    implementation("io.grpc:grpc-protobuf:1.64.0")
    implementation("io.grpc:grpc-stub:1.64.0")

    implementation("com.google.protobuf:protobuf-java:4.27.0")

    implementation("javax.annotation:javax.annotation-api:1.3.2")
}

java.toolchain {
    languageVersion = JavaLanguageVersion.of(21)
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.26.1"
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